//! Independent components and the subsystem of one. Two entities share a component when one
//! action, invariant, or after-assert reads or writes both; `now` joins whatever reads it.
//! Components interleave freely, so the reachable set of the whole system is the product of
//! the components' sets.

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

    fn property_footprint(&self, p: &Property) -> Footprint {
        let mut fp = Footprint::default();
        p.binders.iter().for_each(|e| fp.add(*e));
        match &p.formula {
            Ltl::Always(a) | Ltl::Eventually(a) => self.pred_footprint(a, &mut fp),
            Ltl::LeadsTo(a, b) => {
                self.pred_footprint(a, &mut fp);
                self.pred_footprint(b, &mut fp);
            }
        }
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
        let mut map = Remap {
            ent: vec![None; self.entities.len()],
            var: vec![None; self.vars.len()],
            act: vec![None; self.actions.len()],
        };
        for (i, e) in comp.entities.iter().enumerate() {
            map.ent[e.index()] = Some(EntId::at(i));
        }
        let mut out = TransitionSystem::empty(self.bounds);
        for (i, decl) in self.vars.iter().enumerate() {
            let keep = match decl.kind {
                VarKind::Exists(e)
                | VarKind::Column { entity: e, .. }
                | VarKind::Prop { entity: e, .. } => map.ent[e.index()].is_some(),
                VarKind::Owner { child, parent } => {
                    map.ent[child.index()].is_some() && map.ent[parent.index()].is_some()
                }
                VarKind::Now => comp.now,
            };
            if !keep {
                continue;
            }
            map.var[i] = Some(VarId::at(out.vars.len()));
            let mut decl = decl.clone();
            decl.kind = map.kind(&decl.kind);
            if let Domain::Instance(p) = decl.domain {
                decl.domain = Domain::Instance(map.e(p));
            }
            if decl.kind == VarKind::Now {
                out.now = map.var[i];
            }
            out.vars.push(decl);
        }
        for e in &comp.entities {
            let mut decl = self.entities[e.index()].clone();
            decl.exists = map.v(decl.exists);
            decl.columns = decl.columns.iter().map(|v| map.v(*v)).collect();
            decl.owners = decl
                .owners
                .iter()
                .filter_map(|v| map.var[v.index()])
                .collect();
            out.entities.push(decl);
        }
        let inside = |fp: Footprint| {
            fp.entities.iter().all(|e| map.ent[e.index()].is_some()) && (!fp.now || comp.now)
        };
        let kept: Vec<usize> = (0..self.actions.len())
            .filter(|&i| {
                let fp = self.action_footprint(&self.actions[i]);
                let nonempty = !fp.entities.is_empty() || fp.now;
                nonempty && inside(fp)
            })
            .collect();
        for i in kept {
            map.act[i] = Some(ActId::at(out.actions.len()));
            out.actions.push(map.action(&self.actions[i]));
        }
        for inv in &self.invariants {
            if inside(self.invariant_footprint(inv)) {
                out.invariants.push(Invariant {
                    binders: inv.binders.iter().map(|e| map.e(*e)).collect(),
                    body: map.pred(&inv.body),
                    ..inv.clone()
                });
            }
        }
        for a in &self.after {
            if inside(self.after_footprint(a)) {
                out.after.push(AfterAssert {
                    on: a.on.iter().filter_map(|x| map.act[x.index()]).collect(),
                    binders: a.binders.iter().map(|e| map.e(*e)).collect(),
                    post: map.pred(&a.post),
                    ..a.clone()
                });
            }
        }
        for p in &self.properties {
            if inside(self.property_footprint(p)) {
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
