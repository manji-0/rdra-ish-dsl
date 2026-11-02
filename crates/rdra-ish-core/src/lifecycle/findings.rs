//! What a cone's graph says about an entity's state models. A pure function of the system and
//! the graph, so any engine that produces the graph gets the same findings.

use super::diag::{Blocked, Entry, Exit, StateDiag};
use super::explore::{Graph, Scan};
use super::expr::State;
use super::trace::Trace;
use super::types::{
    ActId, ActionOrigin, Domain, Effect, EntId, ParamRole, Site, Value, VarId, VarKind,
};
use super::TransitionSystem;
use crate::model::ConditionKey;

/// A guarded arm `(from, to, site, conditions)`; a chosen value has no `from`.
type GuardedArm<'a> = (
    Option<Value>,
    Value,
    &'a Site,
    &'a Vec<(ConditionKey, String)>,
);

/// What the graph says about one state of a state model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Witness {
    /// `states[at]` is the first state in discovery order where a present slot holds it.
    At(u32),
    /// No state of a complete graph holds it: a `NoWitness` error.
    Missing(Box<StateDiag>),
    /// No state holds it, but a cap stopped the exploration, so it may still be reachable.
    NotDecided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueWitness {
    pub var: VarId,
    pub value: Value,
    pub witness: Witness,
}

/// What the graph says about one action the entity's cone keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runs {
    /// Some edge carries it, a self-loop included.
    Fired,
    /// No edge of a complete graph carries it: a `NeverRuns` warning.
    Never(Box<StateDiag>),
    /// No edge carries it, but a cap stopped the exploration, so it may still run.
    NotDecided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRuns {
    pub act: ActId,
    pub runs: Runs,
}

/// What the graph says about one `transitions(..).when(Cond)` arm of a use case that runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Taken {
    /// Some edge of the action moves an instance from the arm's `from` to its `to`.
    Yes,
    /// The action runs and some state holds the arm's `from`, but no edge of a complete
    /// graph takes the arm: an `ArmNeverTaken` warning.
    Never(Box<StateDiag>),
    /// The action never runs here or no state holds `from`: `NeverRuns` and the witnesses
    /// speak for the arm.
    Moot,
    /// No edge takes it, but a cap stopped the exploration, so it may still be taken.
    NotDecided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmTaken {
    pub act: ActId,
    /// The arm's `transitions` declaration, or the value's `chooses` line.
    pub site: Site,
    /// `Entity.column`, `from`, and `to` as the model names them. A `chooses` value has no
    /// `from` and its value as `to`.
    pub column: String,
    pub from: Option<String>,
    pub to: String,
    pub taken: Taken,
}

/// What the graph says about one state of a state model that has a way out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Every present instance at it, in every reachable state, has a path that moves it.
    Leaves,
    /// `states[at]` is the first state in discovery order with a present instance no path
    /// moves off it: a `Deadlock`, whose trace the caller fills in from its own run.
    Stuck { at: u32, diag: Box<StateDiag> },
    /// A cap stopped the exploration, so a state it did not expand may still move.
    NotDecided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueProgress {
    pub var: VarId,
    pub value: Value,
    pub progress: Progress,
}

/// A way out of one state, as the system has it: the param it moves, and the state it enters
/// (`None` for a deletion).
struct ExitArm {
    act: ActId,
    param: usize,
    to: Option<Value>,
    site: Site,
}

/// The moves of a graph by state, one direction, self-loops left out.
struct Adjacency {
    start: Vec<u32>,
    next: Vec<u32>,
}

impl Adjacency {
    /// From `n` states and the `(from, to)` pairs `pairs` yields, each time it is called.
    fn new<I: Iterator<Item = (u32, u32)>>(n: usize, pairs: impl Fn() -> I) -> Self {
        let mut start = vec![0u32; n + 1];
        for (from, _) in pairs() {
            start[from as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut next = vec![0u32; start[n] as usize];
        for (from, to) in pairs() {
            next[fill[from as usize] as usize] = to;
            fill[from as usize] += 1;
        }
        Adjacency { start, next }
    }

    fn of(&self, at: u32) -> &[u32] {
        &self.next[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateModelFindings {
    /// Each state of the entity's state models, column by column in declared value order.
    pub witnesses: Vec<ValueWitness>,
    /// Each use case action with a param on the entity that it does not only point at as a
    /// parent, in action order. The entity's cone keeps exactly these, with every guard.
    pub runs: Vec<ActionRuns>,
    /// Each state of the entity's state models that is not an end state, in `witnesses` order.
    pub deadlocks: Vec<ValueProgress>,
    /// Each guarded `transitions` arm on the entity's columns, in action and arm order.
    pub arms: Vec<ArmTaken>,
}

impl TransitionSystem {
    /// The states of `e`'s state models: on each lifecycle column (variation columns are the
    /// actor's choice), the `transitions` endpoints and the values a fresh slot starts at.
    pub(crate) fn state_model_values(&self, e: EntId) -> Vec<(VarId, Value)> {
        let mut out = Vec::new();
        for &var in &self.entity(e).columns {
            let decl = self.var(var);
            let lifecycle = matches!(
                decl.kind,
                VarKind::Column {
                    lifecycle: true,
                    variation: false,
                    ..
                }
            );
            if !lifecycle || !matches!(decl.domain, Domain::Enum(_)) {
                continue;
            }
            let mut values = self.start_values(e, var);
            for action in &self.actions {
                for effect in &action.effects {
                    if let Effect::Transition { var: v, arms, .. } = effect {
                        if *v == var {
                            values.extend(arms.iter().flat_map(|(from, to)| [*from, *to]));
                        }
                    }
                }
            }
            values.sort();
            values.dedup();
            out.extend(values.into_iter().map(|v| (var, v)));
        }
        out
    }

    /// Witnesses of `e`'s state models on `g`, a graph of this system.
    pub fn state_model_findings(&self, e: EntId, g: &Graph) -> StateModelFindings {
        let targets = self.state_model_values(e);
        let mut first: Vec<Option<u32>> = vec![None; targets.len()];
        let mut open = targets.len();
        for (at, s) in g.states.iter().enumerate() {
            if open == 0 {
                break;
            }
            for slot in 0..self.bounds.slots() {
                if !s.is_present(self, e, slot) {
                    continue;
                }
                for (i, (var, value)) in targets.iter().enumerate() {
                    if first[i].is_none() && s.get(self, *var, slot) == *value {
                        first[i] = Some(at as u32);
                        open -= 1;
                    }
                }
            }
        }
        let witnesses = targets
            .iter()
            .copied()
            .zip(first)
            .map(|((var, value), at)| ValueWitness {
                var,
                value,
                witness: match at {
                    Some(at) => Witness::At(at),
                    None if g.complete => {
                        Witness::Missing(Box::new(self.no_witness(e, var, value)))
                    }
                    None => Witness::NotDecided,
                },
            })
            .collect();
        StateModelFindings {
            witnesses,
            runs: self.runs(e, g),
            deadlocks: self.deadlocks(e, &targets, g),
            arms: self.guarded_arms(e, g),
        }
    }

    /// The ways out of `value` on `var` of `e`: each `transitions` arm `value -> w` with
    /// `w != value`, and each deletion of an `e`. A state with no such arm is an end state, a
    /// state whose only arm leads back to itself included.
    fn exits(&self, e: EntId, var: VarId, value: Value) -> Vec<ExitArm> {
        let mut out = Vec::new();
        for (ai, action) in self.actions.iter().enumerate() {
            for effect in &action.effects {
                match effect {
                    Effect::Transition {
                        var: v,
                        param,
                        arms,
                        sites,
                        ..
                    } if *v == var => {
                        for ((from, to), site) in arms.iter().zip(sites) {
                            if *from == value && *to != value {
                                out.push(ExitArm {
                                    act: ActId::at(ai),
                                    param: usize::from(*param),
                                    to: Some(*to),
                                    site: site.clone(),
                                });
                            }
                        }
                    }
                    Effect::Delete { param } if action.params[usize::from(*param)].entity == e => {
                        out.push(ExitArm {
                            act: ActId::at(ai),
                            param: usize::from(*param),
                            to: None,
                            site: action.site.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// For each non-end state `v` of `targets`: whether some reachable state has a present
    /// slot `k` at `v` from which no path changes the column at `k` or deletes `k`. Per column
    /// and slot, the states that can leave are the backward closure of the sources of the
    /// edges that do; linear in the edges.
    fn deadlocks(&self, e: EntId, targets: &[(VarId, Value)], g: &Graph) -> Vec<ValueProgress> {
        let open: Vec<(VarId, Value, Vec<ExitArm>)> = targets
            .iter()
            .map(|&(var, value)| (var, value, self.exits(e, var, value)))
            .filter(|(_, _, exits)| exits.iter().any(|x| x.to.is_some()))
            .collect();
        if !g.complete {
            return open
                .into_iter()
                .map(|(var, value, _)| ValueProgress {
                    var,
                    value,
                    progress: Progress::NotDecided,
                })
                .collect();
        }
        if open.is_empty() {
            return Vec::new();
        }
        let n = g.states.len();
        let moves = || g.edges().filter(|(f, t, _)| f != t).map(|(f, t, _)| (f, t));
        let preds = Adjacency::new(n, || moves().map(|(f, t)| (t, f)));
        let mut succs: Option<Adjacency> = None;
        let present = |at: usize, k: usize| g.states[at].is_present(self, e, k);
        let mut out = Vec::new();
        let mut columns: Vec<(VarId, Vec<Vec<bool>>)> = Vec::new();
        for (var, value, exits) in open {
            if !columns.iter().any(|(v, _)| *v == var) {
                let leave = (0..self.bounds.slots())
                    .map(|k| self.can_leave(e, var, k, g, &preds))
                    .collect();
                columns.push((var, leave));
            }
            let (_, leave) = columns.iter().find(|(v, _)| *v == var).expect("just added");
            let stuck = |at: usize, k: usize| {
                present(at, k) && g.states[at].get(self, var, k) == value && !leave[k][at]
            };
            let first = (0..self.bounds.slots())
                .filter_map(|k| (0..n).find(|&at| stuck(at, k)).map(|at| (at, k)))
                .min();
            let progress = match first {
                None => Progress::Leaves,
                Some((at, k)) => {
                    let succs = succs.get_or_insert_with(|| Adjacency::new(n, moves));
                    let held = Self::forward(succs, at as u32);
                    let held: Vec<&State> = held.iter().map(|&i| &g.states[i as usize]).collect();
                    Progress::Stuck {
                        at: at as u32,
                        diag: Box::new(self.deadlock(e, var, value, k, &exits, &held)),
                    }
                }
            };
            out.push(ValueProgress {
                var,
                value,
                progress,
            });
        }
        out
    }

    /// Which states have a path that changes `var` at slot `k` of `e` or deletes `k`: the
    /// sources of such edges, and, backwards, every state with `k` present that steps into one.
    fn can_leave(&self, e: EntId, var: VarId, k: usize, g: &Graph, preds: &Adjacency) -> Vec<bool> {
        let mut leave = vec![false; g.states.len()];
        let mut stack = Vec::new();
        for (from, to, _) in g.edges() {
            let (s, t) = (&g.states[from as usize], &g.states[to as usize]);
            if leave[from as usize] || !s.is_present(self, e, k) {
                continue;
            }
            if !t.is_present(self, e, k) || t.get(self, var, k) != s.get(self, var, k) {
                leave[from as usize] = true;
                stack.push(from);
            }
        }
        while let Some(at) = stack.pop() {
            for &p in preds.of(at) {
                if !leave[p as usize] && g.states[p as usize].is_present(self, e, k) {
                    leave[p as usize] = true;
                    stack.push(p);
                }
            }
        }
        leave
    }

    /// `at` and every state a path from it reaches.
    fn forward(succs: &Adjacency, at: u32) -> Vec<u32> {
        let mut seen = std::collections::HashSet::from([at]);
        let mut out = vec![at];
        let mut next = 0;
        while next < out.len() {
            for &to in succs.of(out[next]) {
                if seen.insert(to) {
                    out.push(to);
                }
            }
            next += 1;
        }
        out
    }

    /// The `Deadlock` of slot `k` at `value`, each exit with what blocks it over `held`, the
    /// states the stuck instance stays in.
    fn deadlock(
        &self,
        e: EntId,
        var: VarId,
        value: Value,
        k: usize,
        exits: &[ExitArm],
        held: &[&State],
    ) -> StateDiag {
        let exits = exits
            .iter()
            .map(|x| {
                let action = &self.actions[x.act.index()];
                let scan = self.scan_guards(x.act, held.iter().copied(), Some((x.param, k as u8)));
                Exit {
                    action: action.name.clone(),
                    origin: action.origin.clone(),
                    to: x.to.map(|to| self.value_text(var, to)),
                    site: x.site.clone(),
                    why: self.blocked(x.act, scan),
                }
            })
            .collect();
        StateDiag::Deadlock {
            entity: self.entity(e).id.clone(),
            column: self.var_name(var),
            value: self.value_text(var, value),
            at: self.inst(e, k),
            exits,
            trace: Trace::default(),
        }
    }

    /// Whether each action the cone of `e` keeps labels an edge of `g`. `observe` sees a
    /// self-loop as any other move, so a self-loop counts.
    fn runs(&self, e: EntId, g: &Graph) -> Vec<ActionRuns> {
        let mut fired = vec![false; self.actions.len()];
        for (_, _, act) in g.edges() {
            fired[act.index()] = true;
        }
        self.action_ids()
            .filter(|act| {
                let action = &self.actions[act.index()];
                matches!(action.origin, ActionOrigin::UseCase { .. })
                    && action
                        .params
                        .iter()
                        .any(|p| p.entity == e && p.role.is_written())
            })
            .map(|act| ActionRuns {
                act,
                runs: if fired[act.index()] {
                    Runs::Fired
                } else if g.complete {
                    Runs::Never(Box::new(self.never_runs(act, g)))
                } else {
                    Runs::NotDecided
                },
            })
            .collect()
    }

    /// Whether each `.when` arm on a column of `e` labels an edge of `g`: some edge of its
    /// action moves a present instance from `from` to `to`. An arm of an action that never
    /// runs, or from a state no instance holds, is left to `NeverRuns` and the witnesses. A
    /// `.when` value of a `chooses` is taken when some edge of its action leaves a present
    /// instance at the value with its row written (changed, or the edge a self-loop).
    fn guarded_arms(&self, e: EntId, g: &Graph) -> Vec<ArmTaken> {
        let mut out = Vec::new();
        let row = &self.entity(e).columns;
        for act in self.action_ids() {
            let action = &self.actions[act.index()];
            for effect in &action.effects {
                // Each guarded arm: (from, to, site, conditions); a chosen value has no `from`.
                let (var, param, arms): (VarId, u8, Vec<GuardedArm>) = match effect {
                    Effect::Transition {
                        var,
                        param,
                        arms,
                        sites,
                        conds,
                    } => (
                        *var,
                        *param,
                        arms.iter()
                            .zip(sites)
                            .zip(conds)
                            .map(|(((from, to), site), conds)| (Some(*from), *to, site, conds))
                            .collect(),
                    ),
                    Effect::Choose {
                        var,
                        param,
                        values,
                        sites,
                        conds,
                    } => (
                        *var,
                        *param,
                        values
                            .iter()
                            .zip(sites)
                            .zip(conds)
                            .map(|((to, site), conds)| (None, *to, site, conds))
                            .collect(),
                    ),
                    _ => continue,
                };
                if action.params[usize::from(param)].entity != e {
                    continue;
                }
                for (from, to, site, conds) in arms {
                    if conds.is_empty() {
                        continue;
                    }
                    let moves = |s: &State, t: &State| {
                        (0..self.bounds.slots()).any(|k| match from {
                            Some(from) => {
                                s.is_present(self, e, k)
                                    && s.get(self, var, k) == from
                                    && t.get(self, var, k) == to
                            }
                            None => {
                                t.is_present(self, e, k)
                                    && t.get(self, var, k) == to
                                    && (s == t
                                        || !s.is_present(self, e, k)
                                        || row
                                            .iter()
                                            .any(|&c| s.get(self, c, k) != t.get(self, c, k)))
                            }
                        })
                    };
                    let mut fired = false;
                    let mut taken = false;
                    for (a, b, by) in g.edges() {
                        if by != act {
                            continue;
                        }
                        fired = true;
                        if moves(&g.states[a as usize], &g.states[b as usize]) {
                            taken = true;
                            break;
                        }
                    }
                    let at_from = || {
                        let Some(from) = from else { return true };
                        g.states.iter().any(|s| {
                            (0..self.bounds.slots())
                                .any(|k| s.is_present(self, e, k) && s.get(self, var, k) == from)
                        })
                    };
                    let (column, from_text, to_text) = (
                        format!("{}.{}", self.entity(e).id, self.var_name(var)),
                        from.map(|from| self.value_text(var, from)),
                        self.value_text(var, to),
                    );
                    let conditions = conds.iter().map(|(_, id)| id.clone()).collect();
                    let taken = if taken {
                        Taken::Yes
                    } else if !g.complete {
                        Taken::NotDecided
                    } else if fired && at_from() {
                        Taken::Never(Box::new(match &from_text {
                            Some(from) => StateDiag::ArmNeverTaken {
                                action: action.name.clone(),
                                column: column.clone(),
                                from: from.clone(),
                                to: to_text.clone(),
                                conditions,
                                site: site.clone(),
                            },
                            None => StateDiag::ValueNeverChosen {
                                action: action.name.clone(),
                                column: column.clone(),
                                value: to_text.clone(),
                                conditions,
                                site: site.clone(),
                            },
                        }))
                    } else {
                        Taken::Moot
                    };
                    out.push(ArmTaken {
                        act,
                        site: site.clone(),
                        column,
                        from: from_text,
                        to: to_text,
                        taken,
                    });
                }
            }
        }
        out
    }

    /// The `NeverRuns` of `act`: only the checks of `moves` that hold in no state of `g`.
    fn never_runs(&self, act: ActId, g: &Graph) -> StateDiag {
        let action = &self.actions[act.index()];
        let why = self.blocked(act, self.scan_guards(act, g.states, None));
        let unlinked = match why {
            Blocked::Guards(_) => self.unlinked_parents(act),
            _ => Vec::new(),
        };
        StateDiag::NeverRuns {
            action: action.name.clone(),
            origin: action.origin.clone(),
            site: action.site.clone(),
            why,
            unlinked,
        }
    }

    /// `(child, parent)` entity ids for each instance `act` creates under an `.optional()` key
    /// whose parent the action does not bind. The child is created unlinked, so a guard that
    /// reads the parent through that key finds none.
    fn unlinked_parents(&self, act: ActId) -> Vec<(String, String)> {
        let action = &self.actions[act.index()];
        let mut out: Vec<(String, String)> = Vec::new();
        for param in action.params.iter().filter(|p| p.role == ParamRole::New) {
            for &owner in &self.entity(param.entity).owners {
                let var = &self.vars[owner.index()];
                let (VarKind::Owner { parent, .. }, Domain::Instance { optional: true, .. }) =
                    (&var.kind, &var.domain)
                else {
                    continue;
                };
                if action.params.iter().any(|p| p.entity == *parent) {
                    continue;
                }
                let pair = (
                    self.entity(param.entity).id.clone(),
                    self.entity(*parent).id.clone(),
                );
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        out
    }

    /// A scan of `act`'s checks in model terms: entities and roles, guards with their sites.
    fn blocked(&self, act: ActId, scan: Scan) -> Blocked {
        let action = &self.actions[act.index()];
        let need = |p: usize| {
            let param = action.params[p];
            super::Need {
                entity: self.entity(param.entity).id.clone(),
                role: param.role,
                via_api: param.via_api.map(|api| self.api_id(api)),
            }
        };
        match scan {
            Scan::Unbound(params) => Blocked::NoInstance(params.into_iter().map(need).collect()),
            Scan::Apart => Blocked::Apart((0..action.params.len()).map(need).collect()),
            Scan::NeverHeld(guards) => Blocked::Guards(
                guards
                    .into_iter()
                    .map(|i| (action.guards[i].why, action.guards[i].site.clone()))
                    .collect(),
            ),
            Scan::EachHeld => Blocked::Together,
            Scan::Crowded(params) => Blocked::Crowded(params.into_iter().map(need).collect()),
        }
    }

    fn no_witness(&self, e: EntId, var: VarId, value: Value) -> StateDiag {
        let text = |v: Value| match &self.var(var).domain {
            Domain::Enum(names) => names[usize::from(v.0)].clone(),
            _ => v.0.to_string(),
        };
        let mut into = Vec::new();
        for action in &self.actions {
            for effect in &action.effects {
                if let Effect::Transition {
                    var: v,
                    arms,
                    sites,
                    ..
                } = effect
                {
                    if *v != var {
                        continue;
                    }
                    for ((from, to), site) in arms.iter().zip(sites) {
                        if *to == value && *from != value {
                            into.push(Entry {
                                action: action.name.clone(),
                                from: Some(text(*from)),
                                site: site.clone(),
                            });
                        }
                    }
                }
            }
        }
        let base = self.var(var).base;
        for (action, param) in self.creations(e) {
            if !Self::created_value(action, var, param, base).contains(&value) {
                continue;
            }
            let assigned = action.effects.iter().find_map(|effect| match effect {
                Effect::Assign {
                    var: v,
                    param: p,
                    site,
                    ..
                } if *v == var && *p == param => Some(site.clone()),
                _ => None,
            });
            into.push(Entry {
                action: action.name.clone(),
                from: None,
                site: assigned.unwrap_or_else(|| action.site.clone()),
            });
        }
        let VarKind::Column { column, .. } = &self.var(var).kind else {
            unreachable!("a state model is on a column");
        };
        StateDiag::NoWitness {
            entity: self.entity(e).id.clone(),
            column: column.clone(),
            value: text(value),
            into,
        }
    }
}
