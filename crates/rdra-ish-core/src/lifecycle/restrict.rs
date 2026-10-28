//! Independent components and the subsystem of one. Two entities share a component when one
//! action, invariant, or after-assert reads or writes both; `now` joins whatever reads it.
//! Components interleave freely, so the reachable set of the whole system is the product of
//! the components' sets.

use std::collections::BTreeSet;

use super::cone::VarSet;
use super::types::{
    ActId, Action, AfterAssert, Domain, Effect, EntId, Guard, Invariant, Ltl, Pred, Property, Term,
    VarId, VarKind,
};
use super::TransitionSystem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// Ascending.
    pub entities: Vec<EntId>,
    /// The component holds `now` and its tick.
    pub now: bool,
}

/// Entities a footprint touches; `true` in the second slot when it reads or writes `now`.
#[derive(Default)]
struct Footprint {
    entities: Vec<EntId>,
    now: bool,
}

impl Footprint {
    fn add(&mut self, e: EntId) {
        if !self.entities.contains(&e) {
            self.entities.push(e);
        }
    }
}

impl TransitionSystem {
    fn var_entity(&self, var: VarId) -> Option<EntId> {
        match self.var(var).kind {
            VarKind::Exists(e) => Some(e),
            VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => Some(entity),
            VarKind::Owner { child, .. } => Some(child),
            VarKind::Now => None,
        }
    }

    fn pred_footprint(&self, pred: &Pred, fp: &mut Footprint) {
        let term = |t: &Term, fp: &mut Footprint| match t {
            Term::Slot { var, .. } => match self.var_entity(*var) {
                Some(e) => fp.add(e),
                None => fp.now = true,
            },
            Term::Now => fp.now = true,
            Term::Lit(_) | Term::Int(_) | Term::Index(_) => {}
        };
        match pred {
            Pred::True | Pred::False => {}
            Pred::Cmp(a, _, b) => {
                term(a, fp);
                term(b, fp);
            }
            Pred::Not(p) => self.pred_footprint(p, fp),
            Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| self.pred_footprint(p, fp)),
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                fp.add(*entity);
                self.pred_footprint(body, fp);
            }
        }
    }

    fn action_footprint(&self, action: &Action) -> Footprint {
        let mut fp = Footprint::default();
        action.params.iter().for_each(|p| fp.add(p.entity));
        for g in &action.guards {
            self.pred_footprint(&g.pred, &mut fp);
        }
        if action.effects.iter().any(|e| matches!(e, Effect::Tick)) {
            fp.now = true;
        }
        fp
    }

    fn invariant_footprint(&self, inv: &Invariant) -> Footprint {
        let mut fp = Footprint::default();
        inv.binders.iter().for_each(|e| fp.add(*e));
        self.pred_footprint(&inv.body, &mut fp);
        fp
    }

    fn after_footprint(&self, a: &AfterAssert) -> Footprint {
        let mut fp = Footprint::default();
        for act in &a.on {
            let afp = self.action_footprint(&self.actions[act.index()]);
            afp.entities.into_iter().for_each(|e| fp.add(e));
            fp.now |= afp.now;
        }
        a.binders.iter().for_each(|e| fp.add(*e));
        self.pred_footprint(&a.post, &mut fp);
        fp
    }

    /// Connected components of the footprint graph, ordered by their first entity; a
    /// component that holds only the clock comes last.
    pub fn components(&self) -> Vec<Component> {
        let n = self.entities.len();
        let now_node = n;
        let mut parent: Vec<usize> = (0..=n).collect();
        fn find(parent: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while parent[r] != r {
                r = parent[r];
            }
            let mut x = x;
            while parent[x] != r {
                let next = parent[x];
                parent[x] = r;
                x = next;
            }
            r
        }
        let mut join = |fp: Footprint| {
            let mut nodes: Vec<usize> = fp.entities.iter().map(|e| e.index()).collect();
            if fp.now {
                nodes.push(now_node);
            }
            for w in nodes.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                parent[a] = b;
            }
        };
        for a in &self.actions {
            join(self.action_footprint(a));
        }
        for inv in &self.invariants {
            join(self.invariant_footprint(inv));
        }
        for a in &self.after {
            join(self.after_footprint(a));
        }
        let mut groups: Vec<(usize, Component)> = Vec::new();
        for i in 0..n {
            let root = find(&mut parent, i);
            match groups.iter_mut().find(|(r, _)| *r == root) {
                Some((_, c)) => c.entities.push(EntId::at(i)),
                None => groups.push((
                    root,
                    Component {
                        entities: vec![EntId::at(i)],
                        now: false,
                    },
                )),
            }
        }
        if self.now.is_some() {
            let root = find(&mut parent, now_node);
            match groups.iter_mut().find(|(r, _)| *r == root) {
                Some((_, c)) => c.now = true,
                None => groups.push((
                    root,
                    Component {
                        entities: Vec::new(),
                        now: true,
                    },
                )),
            }
        }
        groups.into_iter().map(|(_, c)| c).collect()
    }

    /// The subsystem of one component: its entities' vars, and the actions, invariants,
    /// asserts, and properties that lie inside it.
    pub fn restrict(&self, comp: &Component) -> TransitionSystem {
        let inside = |e: EntId| comp.entities.contains(&e);
        let vars: VarSet = self
            .vars
            .iter()
            .enumerate()
            .filter(|(_, d)| match d.kind {
                VarKind::Exists(e)
                | VarKind::Column { entity: e, .. }
                | VarKind::Prop { entity: e, .. } => inside(e),
                VarKind::Owner { child, parent } => inside(child) && inside(parent),
                VarKind::Now => comp.now,
            })
            .map(|(i, _)| VarId::at(i))
            .collect();
        let keep: BTreeSet<ActId> = (0..self.actions.len())
            .filter(|&i| {
                let fp = self.action_footprint(&self.actions[i]);
                let nonempty = !fp.entities.is_empty() || fp.now;
                nonempty && fp.entities.iter().all(|e| inside(*e)) && (!fp.now || comp.now)
            })
            .map(ActId::at)
            .collect();
        self.restrict_to(&vars, &keep)
    }

    /// The system over `vars` only. An action stays when it writes a var in `vars` or is in
    /// `keep`; it keeps every guard and param, and only its effects on `vars`. Invariants,
    /// asserts, and properties stay when everything they read is in `vars`. For a closed
    /// `vars` (see `cone`) its reachable states are the projection of this system's.
    pub fn restrict_to(&self, vars: &VarSet, keep: &BTreeSet<ActId>) -> TransitionSystem {
        let mut map = Remap {
            ent: vec![None; self.entities.len()],
            var: vec![None; self.vars.len()],
            act: vec![None; self.actions.len()],
        };
        let kept_entities: Vec<usize> = (0..self.entities.len())
            .filter(|&i| vars.contains(&self.entities[i].exists))
            .collect();
        for (n, &i) in kept_entities.iter().enumerate() {
            map.ent[i] = Some(EntId::at(n));
        }
        let mut out = TransitionSystem::empty(self.bounds);
        for &v in vars {
            map.var[v.index()] = Some(VarId::at(out.vars.len()));
            let mut decl = self.vars[v.index()].clone();
            decl.kind = map.kind(&decl.kind);
            if let Domain::Instance { parent, optional } = decl.domain {
                decl.domain = Domain::Instance {
                    parent: map.e(parent),
                    optional,
                };
            }
            if decl.kind == VarKind::Now {
                out.now = map.var[v.index()];
            }
            out.vars.push(decl);
        }
        for &i in &kept_entities {
            let mut decl = self.entities[i].clone();
            decl.exists = map.v(decl.exists);
            decl.columns = decl
                .columns
                .iter()
                .filter_map(|v| map.var[v.index()])
                .collect();
            decl.owners = decl
                .owners
                .iter()
                .filter_map(|v| map.var[v.index()])
                .collect();
            out.entities.push(decl);
        }
        for (i, action) in self.actions.iter().enumerate() {
            let effects: Vec<Effect> = action
                .effects
                .iter()
                .filter(|eff| {
                    self.effect_writes(action, eff)
                        .iter()
                        .any(|v| vars.contains(v))
                })
                .cloned()
                .collect();
            if effects.is_empty() && !keep.contains(&ActId::at(i)) {
                continue;
            }
            let mut action = action.clone();
            action.effects = effects;
            map.act[i] = Some(ActId::at(out.actions.len()));
            out.actions.push(map.action(&action));
        }
        let reads_inside = |preds: &[&Pred], binders: &[EntId]| {
            let mut r = VarSet::new();
            binders.iter().for_each(|e| {
                r.insert(self.entity(*e).exists);
            });
            preds.iter().for_each(|p| self.pred_reads(p, &mut r));
            r.is_subset(vars)
        };
        for inv in &self.invariants {
            if reads_inside(&[&inv.body], &inv.binders) {
                out.invariants.push(Invariant {
                    binders: inv.binders.iter().map(|e| map.e(*e)).collect(),
                    body: map.pred(&inv.body),
                    ..inv.clone()
                });
            }
        }
        for a in &self.after {
            let on: Vec<ActId> = a.on.iter().filter_map(|x| map.act[x.index()]).collect();
            if on.len() == a.on.len() && reads_inside(&[&a.post], &a.binders) {
                out.after.push(AfterAssert {
                    on,
                    binders: a.binders.iter().map(|e| map.e(*e)).collect(),
                    post: map.pred(&a.post),
                    ..a.clone()
                });
            }
        }
        for p in &self.properties {
            let preds: Vec<&Pred> = match &p.formula {
                Ltl::Always(x) | Ltl::Eventually(x) => vec![x],
                Ltl::LeadsTo(x, y) => vec![x, y],
            };
            if reads_inside(&preds, &p.binders) {
                let formula = match &p.formula {
                    Ltl::Always(x) => Ltl::Always(map.pred(x)),
                    Ltl::Eventually(x) => Ltl::Eventually(map.pred(x)),
                    Ltl::LeadsTo(x, y) => Ltl::LeadsTo(map.pred(x), map.pred(y)),
                };
                out.properties.push(Property {
                    binders: p.binders.iter().map(|e| map.e(*e)).collect(),
                    formula,
                    ..p.clone()
                });
            }
        }
        out.layout();
        out
    }
}

struct Remap {
    ent: Vec<Option<EntId>>,
    var: Vec<Option<VarId>>,
    act: Vec<Option<ActId>>,
}

impl Remap {
    fn e(&self, e: EntId) -> EntId {
        self.ent[e.index()].expect("entity outside the component")
    }

    fn v(&self, v: VarId) -> VarId {
        self.var[v.index()].expect("var outside the component")
    }

    fn kind(&self, kind: &VarKind) -> VarKind {
        match kind {
            VarKind::Exists(e) => VarKind::Exists(self.e(*e)),
            VarKind::Column {
                entity,
                column,
                lifecycle,
                variation,
                pg_type,
            } => VarKind::Column {
                entity: self.e(*entity),
                column: column.clone(),
                lifecycle: *lifecycle,
                variation: *variation,
                pg_type: pg_type.clone(),
            },
            VarKind::Prop { entity, key } => VarKind::Prop {
                entity: self.e(*entity),
                key: key.clone(),
            },
            VarKind::Owner { child, parent } => VarKind::Owner {
                child: self.e(*child),
                parent: self.e(*parent),
            },
            VarKind::Now => VarKind::Now,
        }
    }

    fn term(&self, t: &Term) -> Term {
        match t {
            Term::Slot { var, at } => Term::Slot {
                var: self.v(*var),
                at: *at,
            },
            other => other.clone(),
        }
    }

    fn pred(&self, p: &Pred) -> Pred {
        match p {
            Pred::True => Pred::True,
            Pred::False => Pred::False,
            Pred::Cmp(a, op, b) => Pred::Cmp(self.term(a), *op, self.term(b)),
            Pred::Not(x) => Pred::Not(Box::new(self.pred(x))),
            Pred::And(xs) => Pred::And(xs.iter().map(|x| self.pred(x)).collect()),
            Pred::Or(xs) => Pred::Or(xs.iter().map(|x| self.pred(x)).collect()),
            Pred::Forall { entity, body } => Pred::Forall {
                entity: self.e(*entity),
                body: Box::new(self.pred(body)),
            },
            Pred::Exists { entity, body } => Pred::Exists {
                entity: self.e(*entity),
                body: Box::new(self.pred(body)),
            },
        }
    }

    fn action(&self, a: &Action) -> Action {
        let mut a = a.clone();
        for p in &mut a.params {
            p.entity = self.e(p.entity);
        }
        a.guards = a
            .guards
            .iter()
            .map(|g| Guard {
                pred: self.pred(&g.pred),
                ..g.clone()
            })
            .collect();
        for eff in &mut a.effects {
            match eff {
                Effect::Assign { var, .. }
                | Effect::Transition { var, .. }
                | Effect::Choose { var, .. } => *var = self.v(*var),
                Effect::Create { .. } | Effect::Delete { .. } | Effect::Tick => {}
            }
        }
        a
    }
}
