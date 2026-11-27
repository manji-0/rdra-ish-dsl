//! The reference interpreter: a subset of the language read straight from the
//! `SemanticModel`, explored by brute force. It imports nothing from `lifecycle`: no
//! `TransitionSystem`, `Domain`, `Value`, `Pred`, evaluator, move generator, cone, packing or
//! symmetry. Its specification is docs/state-derivation.md ("The transition system") and
//! D1 to D16 of docs/theory/semantics.md, not `lifecycle/build`.
//!
//! A state is a `Vec` per entity of a `Row` per slot, each value a plain enum with `i64`
//! integers and Enum values by name, so no width can wrap and no encoding can merge two
//! values. Everything outside the subset is refused by `RefModel::read` with a reason.
//!
//! In the subset:
//! - entities with Enum, Bool and Int columns (Money, Decimal, Date and DateTime read as
//!   Int), each with or without `@null` and `@default`; a `@null` column of any other type
//!   (null or present); a String column without `@null` has no variable and is never read.
//!   By D2 an Int-typed column is a number only where something writes or compares a number
//!   to it (`numbers`); otherwise it is null or present under `@null`, and has no variable
//!   without it;
//! - `creates`, `updates` and `deletes` of one entity per use case, which is then one
//!   transaction group; an entity no use case creates is pre-existing;
//! - `sets(UC, E.c == v)` with `v` an Enum value, `true`/`false`, an Int in `0..=m`, `null`,
//!   or `present` (one successor per Int in `0..=m` on a nullable Int), with or without
//!   `.when(Cond)`;
//! - `transitions(UC, E.c, a -> b)` on an Enum column, with choices and `.when(Cond)`;
//! - `applies_to(Cond, UC)` whose `holds` body compares columns of the use case's entity;
//! - the rules `forbidden`, `invariant(..).when(..).then(..)`, `required`, `exclusive` on one
//!   entity (with `.when(Cond)`), and `@check`;
//! - comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in` of a column with a
//!   literal, `null`, `present`, or another column of the entity.
//!
//! Out (refused): `relate` and every owner, key and cardinality rule; `@unique`; variations
//! and `chooses`; `drives`; `links`/`relinks`/`unlinks`; copies; `invokes` and APIs;
//! `triggers`; published entities; `@derived`; `now` and the clock; multi-entity rules and
//! quantifiers; `after` assertions; use cases that write two entities (joins, transaction
//! groups); `transitions` on a created row; and temporal properties, which this interpreter
//! does not evaluate (they read no state the subset leaves out, so a model that has them is
//! still compared on its state graph and rules; a property that compares with `now` adds the
//! clock, and the harness skips the model).
//!
//! Readings it shares with the builder, so a fault there is not one this interpreter sees:
//! which Int-typed columns are numbers (`numbers` restates `build/naming.rs`'s rule from the
//! model), the rule names (`Forbidden_<E>_<i>` with `i` the rule's index among all
//! `forbidden` rules, and so on), and the parser and `build_model`, which both sides read.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::model::{
    CheckExpr, CmpOpModel, CmpRhs, ColumnType, ComparisonProp, ConditionKey, CrossCmpRhs,
    CrossConstraintScope, CrossEntityCondition, EffectValue, EntityCondition, EntityKey, NodeRef,
    RelKind, RuleRef, SemanticModel, SetValue, TemporalExpr, TemporalRhs, UseCaseKey,
};

/// One value of a column.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Val {
    Null,
    /// The value of a nullable column of a type the lifecycle reads only as null or not.
    Present,
    Bool(bool),
    Enum(String),
    Int(i64),
}

impl std::fmt::Display for Val {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Val::Null => write!(f, "null"),
            Val::Present => write!(f, "present"),
            Val::Bool(b) => write!(f, "{b}"),
            Val::Enum(s) => write!(f, "{s}"),
            Val::Int(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ex {
    Absent,
    Present,
    Deleted,
}

/// One slot of an entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Row {
    pub ex: Ex,
    /// Per column of the entity, in declaration order.
    pub vals: Vec<Val>,
}

/// Per entity of `RefModel::ents`, per slot.
pub type RState = Vec<Vec<Row>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Enum(Vec<String>),
    Bool,
    Int,
    /// null or present: a `@null` column of a type with no other lifecycle value.
    Opaque,
    /// A String column without `@null`: no variable; reading it is out of the subset.
    Unread,
}

#[derive(Clone, Debug)]
pub struct Col {
    pub name: String,
    pub ty: Ty,
    pub nullable: bool,
    pub base: Val,
}

#[derive(Clone, Debug)]
pub struct Ent {
    pub name: String,
    pub cols: Vec<Col>,
    pub slots: usize,
    pub pre_existing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
enum Rhs {
    Lit(Val),
    /// `in` (`true`) or `not in` (`false`) these values.
    Set(bool, Vec<Val>),
    Col(usize),
}

/// A comparison on one row.
#[derive(Clone, Debug)]
struct Atom {
    col: usize,
    op: Op,
    rhs: Rhs,
}

#[derive(Clone, Debug)]
enum Expr {
    Atom(Atom),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Create,
    Update,
    Delete,
}

/// One value a written column may take, under the guard of its line.
#[derive(Clone, Debug)]
struct Choice {
    val: Val,
    /// Present for a `.when` line or arm; read on the pre-state of an updated row and on
    /// the post-state of a created one.
    guard: Option<Expr>,
    /// For an arm: the value the column must hold before the step.
    from: Option<Val>,
}

#[derive(Clone, Debug)]
struct Act {
    name: String,
    ent: usize,
    kind: Kind,
    guard: Vec<Expr>,
    /// Per written column, the values it may take.
    writes: Vec<(usize, Vec<Choice>)>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub name: String,
    ent: usize,
    body: Expr,
}

/// The subset of a model at one bound.
#[derive(Clone, Debug)]
pub struct RefModel {
    pub ents: Vec<Ent>,
    acts: Vec<Act>,
    pub rules: Vec<Rule>,
}

/// The explored graph.
pub struct Explored {
    pub initial: BTreeSet<RState>,
    pub states: BTreeSet<RState>,
    /// `(from, use case, to)`.
    pub edges: BTreeSet<(RState, String, RState)>,
    /// The rules some reachable state breaks.
    pub violated: BTreeSet<String>,
}

type Why = String;

fn out<T>(why: impl Into<String>) -> Result<T, Why> {
    Err(why.into())
}

impl RefModel {
    /// The subset of `model` with `slots(entity id)` slots per entity and Int values
    /// `0..=int_max`; `Err` names the first construct outside the subset.
    pub fn read(
        model: &SemanticModel,
        slots: impl Fn(&str) -> usize,
        int_max: i64,
    ) -> Result<RefModel, Why> {
        Reader {
            model,
            int_max,
            ent_of: HashMap::new(),
            numeric: numbers(model),
        }
        .read(slots)
    }

    /// Explores every reachable state; `None` past `cap` states.
    pub fn explore(&self, cap: usize) -> Option<Explored> {
        let init: RState = (self.ents.iter())
            .map(|e| {
                let ex = if e.pre_existing {
                    Ex::Present
                } else {
                    Ex::Absent
                };
                let row = Row {
                    ex,
                    vals: e.cols.iter().map(|c| c.base.clone()).collect(),
                };
                vec![row; e.slots]
            })
            .collect();
        let mut seen: BTreeSet<RState> = BTreeSet::new();
        let mut edges = BTreeSet::new();
        let mut queue = VecDeque::new();
        seen.insert(init.clone());
        queue.push_back(init.clone());
        while let Some(s) = queue.pop_front() {
            for act in &self.acts {
                for t in self.successors(act, &s) {
                    if seen.insert(t.clone()) {
                        if seen.len() > cap {
                            return None;
                        }
                        queue.push_back(t.clone());
                    }
                    edges.insert((s.clone(), act.name.clone(), t));
                }
            }
        }
        let violated = (self.rules.iter())
            .filter(|r| {
                seen.iter().any(|s| {
                    (s[r.ent].iter()).any(|row| row.ex == Ex::Present && !holds(&r.body, row))
                })
            })
            .map(|r| r.name.clone())
            .collect();
        Some(Explored {
            initial: BTreeSet::from([init]),
            states: seen,
            edges,
            violated,
        })
    }

    /// Every state one step of `act` leads to from `s`.
    fn successors(&self, act: &Act, s: &RState) -> Vec<RState> {
        let ent = &self.ents[act.ent];
        let rows = &s[act.ent];
        let slots: Vec<usize> = match act.kind {
            // The lowest absent slot (D7): a deleted slot is never reused.
            Kind::Create => rows
                .iter()
                .position(|r| r.ex == Ex::Absent)
                .into_iter()
                .collect(),
            Kind::Update | Kind::Delete => (0..rows.len())
                .filter(|&k| rows[k].ex == Ex::Present)
                .collect(),
        };
        let mut out = Vec::new();
        for k in slots {
            let pre = &rows[k];
            let start = match act.kind {
                Kind::Create => Row {
                    ex: Ex::Present,
                    vals: ent.cols.iter().map(|c| c.base.clone()).collect(),
                },
                Kind::Delete => Row {
                    ex: Ex::Deleted,
                    vals: ent.cols.iter().map(|c| c.base.clone()).collect(),
                },
                Kind::Update => pre.clone(),
            };
            // Each written column's choices, each the product of the others'.
            let mut posts: Vec<(Row, Vec<&Choice>)> = vec![(start, Vec::new())];
            for (col, options) in &act.writes {
                let mut next = Vec::new();
                for (row, taken) in &posts {
                    for o in options {
                        if o.from.as_ref().is_some_and(|f| *f != pre.vals[*col]) {
                            continue;
                        }
                        let mut row = row.clone();
                        row.vals[*col] = o.val.clone();
                        let mut taken = taken.clone();
                        taken.push(o);
                        next.push((row, taken));
                    }
                }
                posts = next;
            }
            for (post, taken) in posts {
                // An updated row's guards read the pre-state, a created row's the post-state.
                let read = if act.kind == Kind::Create { &post } else { pre };
                let guards = act
                    .guard
                    .iter()
                    .chain(taken.iter().filter_map(|o| o.guard.as_ref()));
                if guards.into_iter().all(|g| holds(g, read)) {
                    let mut t = s.clone();
                    t[act.ent][k] = post;
                    out.push(t);
                }
            }
        }
        out
    }

    /// `s` as text: `Item#1{present, n=3}` per slot.
    pub fn show(&self, s: &RState) -> String {
        let mut parts = Vec::new();
        for (e, rows) in self.ents.iter().zip(s) {
            for (k, row) in rows.iter().enumerate() {
                let ex = match row.ex {
                    Ex::Absent => "absent",
                    Ex::Present => "present",
                    Ex::Deleted => "deleted",
                };
                let cols: Vec<String> = (e.cols.iter().zip(&row.vals))
                    .filter(|(c, _)| c.ty != Ty::Unread)
                    .map(|(c, v)| format!("{}={v}", c.name))
                    .collect();
                parts.push(format!("{}#{}{{{ex}, {}}}", e.name, k + 1, cols.join(", ")));
            }
        }
        parts.join(" ")
    }
}

fn holds(e: &Expr, row: &Row) -> bool {
    match e {
        Expr::Atom(a) => atom(a, row),
        Expr::Not(x) => !holds(x, row),
        Expr::And(xs) => xs.iter().all(|x| holds(x, row)),
        Expr::Or(xs) => xs.iter().any(|x| holds(x, row)),
    }
}

/// Language reference, "Null": null equals only null, is unequal to every value, and is
/// never ordered; two columns are equal only when both are present and equal.
fn atom(a: &Atom, row: &Row) -> bool {
    let x = &row.vals[a.col];
    match &a.rhs {
        Rhs::Lit(v) => against(x, a.op, v),
        Rhs::Set(within, vs) => vs.iter().any(|v| equal_lit(x, v)) == *within,
        Rhs::Col(c) => {
            let y = &row.vals[*c];
            let both = *x != Val::Null && *y != Val::Null;
            match a.op {
                Op::Eq => both && x == y,
                Op::Ne => !(both && x == y),
                op => match (x, y) {
                    (Val::Int(p), Val::Int(q)) => ordered(*p, op, *q),
                    _ => false,
                },
            }
        }
    }
}

fn equal_lit(x: &Val, v: &Val) -> bool {
    match v {
        Val::Present => *x != Val::Null,
        v => x == v,
    }
}

fn against(x: &Val, op: Op, v: &Val) -> bool {
    match op {
        Op::Eq => equal_lit(x, v),
        Op::Ne => !equal_lit(x, v),
        op => match (x, v) {
            (Val::Int(p), Val::Int(q)) => ordered(*p, op, *q),
            _ => false,
        },
    }
}

fn ordered(p: i64, op: Op, q: i64) -> bool {
    match op {
        Op::Lt => p < q,
        Op::Le => p <= q,
        Op::Gt => p > q,
        Op::Ge => p >= q,
        Op::Eq => p == q,
        Op::Ne => p != q,
    }
}

struct Reader<'m> {
    model: &'m SemanticModel,
    int_max: i64,
    ent_of: HashMap<EntityKey, usize>,
    /// The Int-typed columns read as numbers (`numbers`).
    numeric: HashSet<(EntityKey, String)>,
}

impl Reader<'_> {
    fn read(mut self, slots: impl Fn(&str) -> usize) -> Result<RefModel, Why> {
        let m = self.model;
        self.refuse_lists()?;
        let writes = self.writes()?;
        let mut ents = Vec::new();
        for (key, e) in &m.entities {
            if !m.publication.is_empty() {
                return out("exposes/shares");
            }
            if !e.unique_constraints.is_empty() {
                return out("@unique");
            }
            let mut cols = Vec::new();
            for c in &e.columns {
                let numeric = self.numeric.contains(&(key, c.name.clone()));
                cols.push(self.column(c, numeric)?);
            }
            let created = writes
                .values()
                .any(|(ek, kind, _)| *ek == key && *kind == Kind::Create);
            self.ent_of.insert(key, ents.len());
            ents.push(Ent {
                name: e.id.clone(),
                cols,
                slots: slots(&e.id),
                pre_existing: !created,
            });
        }
        let mut acts = Vec::new();
        for (uc, (ek, kind, guards)) in &writes {
            acts.push(self.action(&ents, *uc, *ek, *kind, guards)?);
        }
        let rules = self.rules(&ents)?;
        Ok(RefModel { ents, acts, rules })
    }

    /// Every model list the subset leaves out is empty.
    fn refuse_lists(&self) -> Result<(), Why> {
        let m = self.model;
        let lists = [
            ("drives", m.proposition_effects.len()),
            ("chooses", m.column_choices.len()),
            ("links/relinks/unlinks", m.links.len()),
            ("a quantifier rule", m.quantifier_constraints.len()),
            (
                "a multi-entity forbidden",
                m.cross_forbidden_constraints.len(),
            ),
            ("a multi-entity invariant", m.cross_entity_invariants.len()),
            ("after(..).assert", m.temporal_assertions.len()),
            ("counted triggers", m.counted_triggers.len()),
            ("an .apart() write", m.apart_writes.len()),
            ("a foreign key", m.foreign_keys().count()),
            ("a junction", m.junction_keys.len()),
        ];
        match lists.iter().find(|(_, n)| *n > 0) {
            Some((why, _)) => out(*why),
            None => Ok(()),
        }
    }

    /// Per use case that writes: its one entity, how, and its `applies_to` guards.
    #[allow(clippy::type_complexity)]
    fn writes(&self) -> Result<BTreeMap<UseCaseKey, (EntityKey, Kind, Vec<ConditionKey>)>, Why> {
        let m = self.model;
        let mut crud: BTreeMap<UseCaseKey, Vec<(EntityKey, Kind)>> = BTreeMap::new();
        let mut guards: BTreeMap<UseCaseKey, Vec<ConditionKey>> = BTreeMap::new();
        for r in &m.relations {
            let kind = match r.kind {
                RelKind::Creates => Kind::Create,
                RelKind::Updates => Kind::Update,
                RelKind::Deletes => Kind::Delete,
                RelKind::AppliesTo => {
                    match (&r.from, &r.to) {
                        (NodeRef::Condition(c), NodeRef::UseCase(uc)) => {
                            guards.entry(*uc).or_default().push(*c)
                        }
                        (NodeRef::Condition(_), NodeRef::System(_)) => {}
                        _ => return out("applies_to on something other than a use case"),
                    }
                    continue;
                }
                RelKind::Triggers => return out("triggers"),
                RelKind::Invokes => return out("invokes"),
                RelKind::Compensates => return out("compensates"),
                RelKind::RelateOneToOne
                | RelKind::RelateOneToMany
                | RelKind::RelateManyToOne
                | RelKind::RelateManyToMany => return out("relate"),
                // Lifecycle facts read from their own lists, or no lifecycle fact at all.
                RelKind::Transitions
                | RelKind::Guards
                | RelKind::Combines
                | RelKind::Raises
                | RelKind::Performs
                | RelKind::Reads
                | RelKind::Displays
                | RelKind::Shows
                | RelKind::Navigates
                | RelKind::Contains
                | RelKind::Belongs
                | RelKind::HasPermission
                | RelKind::RequiresPermission
                | RelKind::RequiresMedium
                | RelKind::Motivates
                | RelKind::Achieves
                | RelKind::Decides
                | RelKind::Precedes
                | RelKind::Branches
                | RelKind::Excepts
                | RelKind::Repeats
                | RelKind::Covers
                | RelKind::Request
                | RelKind::Response
                | RelKind::ErrorResponse
                | RelKind::Qualifies
                | RelKind::Constrains
                | RelKind::MapsField
                | RelKind::Owns => continue,
            };
            match (&r.from, &r.to) {
                (NodeRef::UseCase(uc), NodeRef::Entity(e)) => {
                    crud.entry(*uc).or_default().push((*e, kind))
                }
                _ => return out("a create/update/delete not from a use case to an entity"),
            }
        }
        for ce in &m.column_effects {
            crud.entry(ce.origin)
                .or_default()
                .push((ce.entity, Kind::Update));
        }
        for st in &m.state_transitions {
            crud.entry(st.usecase)
                .or_default()
                .push((st.entity, Kind::Update));
        }
        let mut writes = BTreeMap::new();
        for (uc, ws) in crud {
            let ents: BTreeSet<EntityKey> = ws.iter().map(|(e, _)| *e).collect();
            if ents.len() != 1 {
                return out("a use case that writes two entities");
            }
            let creates = ws.iter().any(|(_, k)| *k == Kind::Create);
            let deletes = ws.iter().any(|(_, k)| *k == Kind::Delete);
            let kind = match (creates, deletes) {
                (true, true) => return out("a use case that creates and deletes"),
                (true, false) => Kind::Create,
                (false, true) => Kind::Delete,
                (false, false) => Kind::Update,
            };
            let g = guards.remove(&uc).unwrap_or_default();
            writes.insert(uc, (ws[0].0, kind, g));
        }
        if guards.keys().any(|uc| !m.use_cases.contains_key(*uc)) {
            return out("a guard on an unknown use case");
        }
        Ok(writes)
    }

    fn column(&self, c: &crate::model::ModelColumn, numeric: bool) -> Result<Col, Why> {
        if c.derived_expr.is_some() {
            return out("@derived");
        }
        if c.is_unique && !c.is_pk {
            return out("@unique");
        }
        if c.default_fn.is_some() || c.on_update.is_some() || c.is_version {
            return out("@default(now()), @on_update or @version");
        }
        let ty = match &c.col_type {
            ColumnType::Int
            | ColumnType::Money
            | ColumnType::Decimal
            | ColumnType::Date
            | ColumnType::DateTime
                if numeric =>
            {
                Ty::Int
            }
            // D2: an Int-typed column nothing reads as a number is null or present.
            ColumnType::Int
            | ColumnType::Money
            | ColumnType::Decimal
            | ColumnType::Date
            | ColumnType::DateTime
                if c.is_nullable =>
            {
                Ty::Opaque
            }
            ColumnType::Int
            | ColumnType::Money
            | ColumnType::Decimal
            | ColumnType::Date
            | ColumnType::DateTime => Ty::Unread,
            ColumnType::Bool => Ty::Bool,
            ColumnType::Enum(vs) => Ty::Enum(vs.clone()),
            ColumnType::String if c.is_nullable => Ty::Opaque,
            ColumnType::String => Ty::Unread,
            ColumnType::Variation(_) => return out("a variation column"),
        };
        let base = match (&c.default_val, c.is_nullable) {
            _ if ty == Ty::Unread => Val::Null,
            (Some(d), _) if d == "null" => Val::Null,
            (Some(_), _) if ty == Ty::Opaque => Val::Present,
            (Some(d), _) => self
                .literal(&ty, c.is_nullable, &parse_default(d))?
                .ok_or_else(|| format!("@default({d}) on {}", c.name))?,
            (None, true) => Val::Null,
            (None, false) => match &ty {
                Ty::Enum(vs) => Val::Enum(vs[0].clone()),
                Ty::Bool => Val::Bool(false),
                Ty::Int => Val::Int(0),
                Ty::Opaque | Ty::Unread => Val::Null,
            },
        };
        if !c.check_constraints.is_empty() && ty == Ty::Unread {
            return out("@check on a String column");
        }
        Ok(Col {
            name: c.name.clone(),
            ty,
            nullable: c.is_nullable,
            base,
        })
    }

    /// The value a literal stands for in a column of `ty`; `None` when it is not one of the
    /// column's values (`present` is not a single value).
    fn literal(&self, ty: &Ty, nullable: bool, v: &EffectValue) -> Result<Option<Val>, Why> {
        Ok(match (ty, v) {
            (_, EffectValue::Null) => nullable.then_some(Val::Null),
            (Ty::Enum(vs), EffectValue::EnumVariant(name)) => enum_value(vs, name).map(Val::Enum),
            (Ty::Bool, EffectValue::Bool(b)) => Some(Val::Bool(*b)),
            (Ty::Int, EffectValue::Int(n)) => {
                (0..=self.int_max).contains(n).then_some(Val::Int(*n))
            }
            (Ty::Unread, _) => return out("a column with no variable is read"),
            _ => None,
        })
    }

    /// A literal of a comparison, which may lie outside the column's values.
    fn compared(&self, ty: &Ty, v: &EffectValue) -> Result<Val, Why> {
        Ok(match (ty, v) {
            (Ty::Unread, _) => return out("a column with no variable is read"),
            (_, EffectValue::Null) => Val::Null,
            (_, EffectValue::Present) => Val::Present,
            (Ty::Enum(vs), EffectValue::EnumVariant(name)) => match enum_value(vs, name) {
                Some(v) => Val::Enum(v),
                None => return out(format!("{name} is no value of its column")),
            },
            (Ty::Bool, EffectValue::Bool(b)) => Val::Bool(*b),
            (Ty::Int, EffectValue::Int(n)) => Val::Int(*n),
            _ => return out(format!("a comparison with {v:?} of another type")),
        })
    }

    fn col_index(&self, ents: &[Ent], e: usize, name: &str) -> Result<usize, Why> {
        match ents[e].cols.iter().position(|c| c.name == name) {
            Some(i) if ents[e].cols[i].ty == Ty::Unread => out("a column with no variable is read"),
            Some(i) => Ok(i),
            None => out(format!("{name} is no column of {}", ents[e].name)),
        }
    }

    fn cmp(
        &self,
        ents: &[Ent],
        e: usize,
        lhs: &str,
        op: CmpOpModel,
        rhs: Rhs0<'_>,
    ) -> Result<Expr, Why> {
        let col = self.col_index(ents, e, lhs)?;
        let ty = &ents[e].cols[col].ty;
        let op1 = match op {
            CmpOpModel::Eq | CmpOpModel::In => Op::Eq,
            CmpOpModel::Ne | CmpOpModel::NotIn => Op::Ne,
            CmpOpModel::Lt => Op::Lt,
            CmpOpModel::Le => Op::Le,
            CmpOpModel::Gt => Op::Gt,
            CmpOpModel::Ge => Op::Ge,
        };
        if op.is_ordered() && *ty != Ty::Int {
            return out("an ordered comparison of a column that is not Int");
        }
        let rhs = match rhs {
            Rhs0::Value(v) => Rhs::Lit(self.compared(ty, v)?),
            Rhs0::Int(n) if *ty == Ty::Int => Rhs::Lit(Val::Int(n)),
            Rhs0::Int(_) => return out("an Int literal compared with a column that is not Int"),
            Rhs0::Values(vs) => {
                let vals = vs
                    .iter()
                    .map(|v| self.compared(ty, v))
                    .collect::<Result<_, _>>()?;
                Rhs::Set(op == CmpOpModel::In, vals)
            }
            Rhs0::Column(c) => {
                let other = self.col_index(ents, e, c)?;
                if ents[e].cols[other].ty != *ty || *ty == Ty::Opaque {
                    return out("a comparison of two columns of different types");
                }
                Rhs::Col(other)
            }
            Rhs0::Now => return out("now"),
        };
        if matches!(rhs, Rhs::Set(..)) != matches!(op, CmpOpModel::In | CmpOpModel::NotIn) {
            return out("in/not in without a value list");
        }
        Ok(Expr::Atom(Atom { col, op: op1, rhs }))
    }

    fn prop(&self, ents: &[Ent], e: usize, p: &ComparisonProp) -> Result<Expr, Why> {
        let rhs = match &p.rhs {
            CmpRhs::Column(c) => Rhs0::Column(c),
            CmpRhs::IntLit(n) => Rhs0::Int(*n),
            CmpRhs::Now => Rhs0::Now,
            CmpRhs::Value(v) => Rhs0::Value(v),
            CmpRhs::Values(vs) => Rhs0::Values(vs),
        };
        self.cmp(ents, e, &p.lhs_column, p.op, rhs)
    }

    fn local(&self, ents: &[Ent], e: usize, conds: &[EntityCondition]) -> Result<Vec<Expr>, Why> {
        conds
            .iter()
            .map(|c| match c {
                EntityCondition::Equals { column, value } => {
                    self.cmp(ents, e, column, CmpOpModel::Eq, Rhs0::Value(value))
                }
                EntityCondition::Comparison(p) => self.prop(ents, e, p),
            })
            .collect()
    }

    /// The comparisons of `cond`'s `holds` body, every one on entity `e`.
    fn body(&self, ents: &[Ent], e: usize, cond: ConditionKey) -> Result<Vec<Expr>, Why> {
        let Some(body) = &self.model.conditions[cond].body else {
            return Ok(Vec::new());
        };
        if !body.quantifiers.is_empty()
            || !body.by.is_empty()
            || !matches!(body.along, CrossConstraintScope::GlobalProduct)
        {
            return out("a holds body with .has/.none/.along/.role");
        }
        let on = |k: EntityKey| -> Result<(), Why> {
            match self.ent_of.get(&k) {
                Some(&x) if x == e => Ok(()),
                _ => out("a holds body that reads another entity"),
            }
        };
        let mut atoms = Vec::new();
        for c in &body.conds {
            atoms.push(match c {
                CrossEntityCondition::Equals { column, value } => {
                    on(column.entity)?;
                    self.cmp(ents, e, &column.column, CmpOpModel::Eq, Rhs0::Value(value))?
                }
                CrossEntityCondition::Comparison(p) => {
                    on(p.lhs.entity)?;
                    let rhs = match &p.rhs {
                        CrossCmpRhs::Column(c) => {
                            on(c.entity)?;
                            Rhs0::Column(&c.column)
                        }
                        CrossCmpRhs::IntLit(n) => Rhs0::Int(*n),
                        CrossCmpRhs::Now => Rhs0::Now,
                        CrossCmpRhs::Value(v) => Rhs0::Value(v),
                        CrossCmpRhs::Values(vs) => Rhs0::Values(vs),
                    };
                    self.cmp(ents, e, &p.lhs.column, p.op, rhs)?
                }
                CrossEntityCondition::Mark { .. } => return out("a run or raised mark"),
            });
        }
        Ok(atoms)
    }

    fn guard(
        &self,
        ents: &[Ent],
        e: usize,
        cond: Option<ConditionKey>,
    ) -> Result<Option<Expr>, Why> {
        cond.map(|c| self.body(ents, e, c).map(Expr::And))
            .transpose()
    }

    fn action(
        &self,
        ents: &[Ent],
        uc: UseCaseKey,
        ek: EntityKey,
        kind: Kind,
        guards: &[ConditionKey],
    ) -> Result<Act, Why> {
        let m = self.model;
        let e = self.ent_of[&ek];
        let mut guard = Vec::new();
        for c in guards {
            guard.extend(self.body(ents, e, *c)?);
        }
        let mut writes: BTreeMap<usize, Vec<Choice>> = BTreeMap::new();
        let mut sets: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
        for ce in m.column_effects.iter().filter(|ce| ce.origin == uc) {
            let SetValue::Value(v) = &ce.value else {
                return out("a copy");
            };
            let col = self.col_index(ents, e, &ce.column)?;
            let c = &ents[e].cols[col];
            let vals = match (v, &c.ty, c.nullable) {
                (EffectValue::Present, Ty::Int, true) => (0..=self.int_max).map(Val::Int).collect(),
                (EffectValue::Present, Ty::Opaque, true) => vec![Val::Present],
                (EffectValue::Present, ..) => {
                    return out("present on a column that is not a nullable Int or String")
                }
                (v, ty, nullable) => match self.literal(ty, nullable, v)? {
                    Some(x) => vec![x],
                    None => {
                        return out(format!(
                            "sets {} == {v:?}: no value of the column at this bound",
                            ce.column
                        ))
                    }
                },
            };
            if kind == Kind::Delete {
                return out("sets on a deleted row");
            }
            let g = self.guard(ents, e, ce.guard)?;
            sets.entry(col).or_default().push(g.is_some());
            let options = writes.entry(col).or_default();
            options.extend(vals.into_iter().map(|val| Choice {
                val,
                guard: g.clone(),
                from: None,
            }));
        }
        if sets
            .values()
            .any(|lines| lines.len() > 1 && lines.contains(&false))
        {
            return out("an unguarded sets beside another sets of the column");
        }
        for st in m.state_transitions.iter().filter(|st| st.usecase == uc) {
            if kind != Kind::Update {
                return out("transitions on a created or deleted row");
            }
            let col = self.col_index(ents, e, &st.column)?;
            if sets.contains_key(&col) {
                return out("transitions and sets on one column");
            }
            let Ty::Enum(vs) = &ents[e].cols[col].ty else {
                return out("transitions on a column that is not an Enum");
            };
            let (Some(from), Some(to)) = (enum_value(vs, &st.from), enum_value(vs, &st.to)) else {
                return out("a transitions arm outside the column's values");
            };
            writes.entry(col).or_default().push(Choice {
                val: Val::Enum(to),
                guard: self.guard(ents, e, st.guard)?,
                from: Some(Val::Enum(from)),
            });
        }
        Ok(Act {
            name: m.use_cases[uc].id.clone(),
            ent: e,
            kind,
            guard,
            writes: writes.into_iter().collect(),
        })
    }

    fn rules(&self, ents: &[Ent]) -> Result<Vec<Rule>, Why> {
        let m = self.model;
        let mut rules = Vec::new();
        let ent = |k: EntityKey| self.ent_of[&k];
        let when = |r: RuleRef, e: usize| -> Result<Vec<Expr>, Why> {
            let mut out = Vec::new();
            for g in m.rule_guards.iter().filter(|g| g.rule == r) {
                out.extend(self.body(ents, e, g.condition)?);
            }
            Ok(out)
        };
        for (i, r) in m.forbidden_constraints.iter().enumerate() {
            let e = ent(r.entity);
            let mut conds = self.local(ents, e, &r.conditions)?;
            conds.extend(when(RuleRef::Forbidden(i), e)?);
            rules.push(Rule {
                name: format!("Forbidden_{}_{i}", ents[e].name),
                ent: e,
                body: Expr::Not(Box::new(Expr::And(conds))),
            });
        }
        for (i, r) in m.entity_invariants.iter().enumerate() {
            let e = ent(r.entity);
            let mut guards = self.local(ents, e, &r.guards)?;
            guards.extend(when(RuleRef::Invariant(i), e)?);
            let then = self.local(ents, e, &r.requireds)?;
            rules.push(Rule {
                name: format!("Inv_{}_{i}", ents[e].name),
                ent: e,
                body: Expr::Or(vec![
                    Expr::Not(Box::new(Expr::And(guards))),
                    Expr::And(then),
                ]),
            });
        }
        for (i, r) in m.required_constraints.iter().enumerate() {
            let e = ent(r.entity);
            rules.push(Rule {
                name: format!("Required_{}_{i}", ents[e].name),
                ent: e,
                body: Expr::And(self.local(ents, e, &r.conditions)?),
            });
        }
        for (i, r) in m.exclusive_constraints.iter().enumerate() {
            let e = ent(r.entity);
            let conds = self.local(ents, e, &r.conditions)?;
            let mut pairs = Vec::new();
            for x in 0..conds.len() {
                for y in x + 1..conds.len() {
                    pairs.push(Expr::And(vec![conds[x].clone(), conds[y].clone()]));
                }
            }
            rules.push(Rule {
                name: format!("Exclusive_{}_{i}", ents[e].name),
                ent: e,
                body: Expr::Not(Box::new(Expr::Or(pairs))),
            });
        }
        for (key, entity) in &m.entities {
            let e = ent(key);
            for c in &entity.columns {
                for (i, check) in c.check_constraints.iter().enumerate() {
                    rules.push(Rule {
                        name: format!("Check_{}_{}_{i}", entity.id, c.name),
                        ent: e,
                        body: self.check(ents, e, &check.expr)?,
                    });
                }
            }
        }
        Ok(rules)
    }

    fn check(&self, ents: &[Ent], e: usize, x: &CheckExpr) -> Result<Expr, Why> {
        Ok(match x {
            CheckExpr::Cmp(p) => self.prop(ents, e, p)?,
            CheckExpr::Not(a) => Expr::Not(Box::new(self.check(ents, e, a)?)),
            CheckExpr::And(a, b) => {
                Expr::And(vec![self.check(ents, e, a)?, self.check(ents, e, b)?])
            }
            CheckExpr::Or(a, b) => Expr::Or(vec![self.check(ents, e, a)?, self.check(ents, e, b)?]),
        })
    }
}

/// The right-hand side of a comparison, from either model form.
enum Rhs0<'a> {
    Value(&'a EffectValue),
    Int(i64),
    Values(&'a [EffectValue]),
    Column(&'a str),
    Now,
}

/// The declared value `name` names, matched exactly or else ignoring ASCII case.
fn enum_value(values: &[String], name: &str) -> Option<String> {
    (values.iter().find(|v| *v == name))
        .or_else(|| values.iter().find(|v| v.eq_ignore_ascii_case(name)))
        .cloned()
}

/// A `@default` as written: `true`, `false`, an integer, or an Enum value.
fn parse_default(d: &str) -> EffectValue {
    match d {
        "true" => EffectValue::Bool(true),
        "false" => EffectValue::Bool(false),
        _ => match d.parse::<i64>() {
            Ok(n) => EffectValue::Int(n),
            Err(_) => EffectValue::EnumVariant(d.to_string()),
        },
    }
}

/// The Int-typed columns something reads or writes as a number: an Int literal written or
/// compared, an ordered comparison, or a comparison with another Int-typed column, in a
/// `sets`, a rule, a `@check`, an applied or `.when` condition body, or a property. D2 makes
/// every other `@null` Int-typed column null or present. The reading is `build/naming.rs`'s,
/// written again here from the model; the reference does not call it.
fn numbers(m: &SemanticModel) -> HashSet<(EntityKey, String)> {
    let mut out: HashSet<(EntityKey, String)> = HashSet::new();
    let mut num = |e: EntityKey, col: &str| {
        out.insert((e, col.to_string()));
    };
    let int_typed = |e: EntityKey, col: &str| {
        m.entities[e].columns.iter().any(|c| {
            c.name == col
                && matches!(
                    c.col_type,
                    ColumnType::Int
                        | ColumnType::Money
                        | ColumnType::Decimal
                        | ColumnType::Date
                        | ColumnType::DateTime
                )
        })
    };
    for ce in &m.column_effects {
        if matches!(&ce.value, SetValue::Value(EffectValue::Int(_))) {
            num(ce.entity, &ce.column);
        }
    }
    let rules = (m.forbidden_constraints.iter())
        .chain(&m.required_constraints)
        .chain(&m.exclusive_constraints)
        .map(|r| (r.entity, &r.conditions))
        .chain(
            (m.entity_invariants.iter())
                .flat_map(|r| [(r.entity, &r.guards), (r.entity, &r.requireds)]),
        );
    for (e, conds) in rules {
        for c in conds {
            match c {
                EntityCondition::Equals { column, value } if is_int(value) => num(e, column),
                EntityCondition::Equals { .. } => {}
                EntityCondition::Comparison(p) => local_prop(e, p, &mut num),
            }
        }
    }
    for (e, entity) in &m.entities {
        for c in &entity.columns {
            for check in &c.check_constraints {
                for p in check.expr.comparisons() {
                    local_prop(e, p, &mut num);
                }
            }
        }
    }
    let applied = (m.relations.iter()).filter_map(|r| match (&r.kind, &r.from) {
        (RelKind::AppliesTo, NodeRef::Condition(c)) => Some(*c),
        _ => None,
    });
    let whens = (m.column_effects.iter().filter_map(|c| c.guard))
        .chain(m.state_transitions.iter().filter_map(|t| t.guard))
        .chain(m.rule_guards.iter().map(|g| g.condition));
    for ck in applied.chain(whens) {
        let Some(body) = &m.conditions[ck].body else {
            continue;
        };
        for c in body.read_conditions() {
            match c {
                CrossEntityCondition::Equals { column, value } if is_int(value) => {
                    num(column.entity, &column.column)
                }
                CrossEntityCondition::Comparison(p) => match &p.rhs {
                    CrossCmpRhs::Column(o) => {
                        for (e, col) in [(p.lhs.entity, &p.lhs.column), (o.entity, &o.column)] {
                            if p.op.is_ordered() || int_typed(e, col) {
                                num(e, col);
                            }
                        }
                    }
                    CrossCmpRhs::Value(v) if !is_int(v) => {}
                    CrossCmpRhs::Values(vs) if !vs.iter().any(is_int) => {}
                    _ => num(p.lhs.entity, &p.lhs.column),
                },
                _ => {}
            }
        }
    }
    let mut stack: Vec<&TemporalExpr> = (m.temporal_properties.iter())
        .flat_map(|p| p.formula.exprs())
        .collect();
    while let Some(x) = stack.pop() {
        match x {
            TemporalExpr::Atom(a) => match &a.rhs {
                TemporalRhs::Value(v) if !a.op.is_ordered() && !is_int(v) => {}
                TemporalRhs::Values(vs) if !vs.iter().any(is_int) => {}
                TemporalRhs::Column { entity, column } => {
                    if a.op.is_ordered() {
                        num(a.entity, &a.column);
                        num(*entity, column);
                    }
                }
                _ => num(a.entity, &a.column),
            },
            TemporalExpr::Not(y) => stack.push(y),
            TemporalExpr::And(y, z) | TemporalExpr::Or(y, z) => {
                stack.push(y);
                stack.push(z);
            }
        }
    }
    out
}

fn is_int(v: &EffectValue) -> bool {
    matches!(v, EffectValue::Int(_))
}

/// `numbers` for one comparison of a rule or `@check` on entity `e`.
fn local_prop(e: EntityKey, p: &ComparisonProp, num: &mut impl FnMut(EntityKey, &str)) {
    match &p.rhs {
        CmpRhs::Value(v) if !is_int(v) => {}
        CmpRhs::Values(vs) if !vs.iter().any(is_int) => {}
        CmpRhs::Column(c) => {
            num(e, &p.lhs_column);
            num(e, c);
        }
        _ => num(e, &p.lhs_column),
    }
}
