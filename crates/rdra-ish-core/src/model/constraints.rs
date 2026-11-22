use super::comparison::{CmpOpModel, ComparisonProp};
use super::effects::EffectValue;
use super::foreign_key::FkId;
use super::keys::{EntityKey, UseCaseKey};

/// One condition of a single-entity rule, on a column of the rule's entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityCondition {
    /// `column == value`.
    Equals {
        column: std::string::String,
        value: EffectValue,
    },
    /// A typed comparison such as `total > 0` or `status != paid`.
    Comparison(ComparisonProp),
}

impl EntityCondition {
    /// The `column == value` conditions of `conditions`, in order.
    pub fn equals(conditions: &[Self]) -> impl Iterator<Item = (&str, &EffectValue)> {
        conditions.iter().filter_map(|c| match c {
            Self::Equals { column, value } => Some((column.as_str(), value)),
            Self::Comparison(_) => None,
        })
    }

    /// The comparisons of `conditions`, in order.
    pub fn comparisons(conditions: &[Self]) -> impl Iterator<Item = &ComparisonProp> {
        conditions.iter().filter_map(|c| match c {
            Self::Comparison(prop) => Some(prop),
            Self::Equals { .. } => None,
        })
    }
}

/// A `forbidden`, `required` or `exclusive` rule on one entity. Which one it is follows from
/// the model list that holds it:
/// - `forbidden(Entity, ...)`: a reached state where every condition holds is a violation.
/// - `required(Entity, ...)`: a reached state where some condition fails is a violation.
/// - `exclusive(Entity, ...)`: a reached state where two or more conditions hold is a violation.
#[derive(Debug, Clone)]
pub struct EntityRule {
    pub entity: EntityKey,
    pub conditions: Vec<EntityCondition>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

impl EntityRule {
    /// The `column == value` conditions, in order.
    pub fn equals(&self) -> impl Iterator<Item = (&str, &EffectValue)> {
        EntityCondition::equals(&self.conditions)
    }

    /// The comparisons, in order.
    pub fn comparisons(&self) -> impl Iterator<Item = &ComparisonProp> {
        EntityCondition::comparisons(&self.conditions)
    }
}

/// `invariant(Entity).when(...).then(...)`: in a reached state where every guard holds, every
/// required condition holds too.
#[derive(Debug, Clone)]
pub struct EntityInvariant {
    pub entity: EntityKey,
    pub guards: Vec<EntityCondition>,
    pub requireds: Vec<EntityCondition>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

/// A column reference resolved to a concrete entity and one of its columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedModelColumnRef {
    pub entity: EntityKey,
    pub column: std::string::String,
}

/// Right-hand side of a cross-entity comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossCmpRhs {
    /// Column reference on the same or another entity.
    Column(QualifiedModelColumnRef),
    /// Integer literal.
    IntLit(i64),
    /// Built-in temporal reference `now`.
    Now,
    /// A value of the left-hand column's type (`Order.status != paid`).
    Value(EffectValue),
    /// The values of `in` / `not in` (`Order.status in (open, paid)`).
    Values(Vec<EffectValue>),
}

/// A comparison proposition that may reference columns on multiple entities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossComparisonProp {
    pub lhs: QualifiedModelColumnRef,
    pub op: CmpOpModel,
    pub rhs: CrossCmpRhs,
}

/// One condition inside a cross-entity constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossEntityCondition {
    /// `Entity.column == value`, written as `Entity.column == value`.
    Equals {
        column: QualifiedModelColumnRef,
        value: EffectValue,
    },
    /// A typed comparison expression such as `Order.total > Payment.amount`.
    Comparison(CrossComparisonProp),
}

/// How a cross-entity constraint should choose entity combinations to evaluate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossConstraintScope {
    /// Evaluate the cross-product of each participating entity's reached patterns.
    GlobalProduct,
    /// `.along(A, B, ..)`: only instances connected by the keys between neighbours.
    RelationPath {
        path: Vec<EntityKey>,
        /// `.role(role)`: on a hop that two keys join, the one with this role.
        by: Vec<String>,
        /// The key joining `path[i]` and `path[i + 1]`. `None` when no key joins them, or
        /// when the hop is ambiguous (reported as `AmbiguousOwnerLink`).
        links: Vec<Option<FkId>>,
    },
}

/// `forbidden(EntityA, EntityB, ...)`.
#[derive(Debug, Clone)]
pub struct CrossForbiddenConstraint {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    /// `.global()`: the rule is meant for every combination of rows, though keys relate them.
    pub global: bool,
    pub conditions: Vec<CrossEntityCondition>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

/// `invariant(EntityA, EntityB).when(...).then(...)`.
#[derive(Debug, Clone)]
pub struct CrossEntityInvariant {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    /// `.global()`: the rule is meant for every combination of rows, though keys relate them.
    pub global: bool,
    pub guards: Vec<CrossEntityCondition>,
    pub requireds: Vec<CrossEntityCondition>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

/// `after(UseCase).assert(...)` で宣言される時相アンカー制約。
#[derive(Debug, Clone)]
pub struct TemporalAssertion {
    pub anchor: UseCaseKey,
    pub scope: Vec<EntityKey>,
    pub requireds: Vec<CrossEntityCondition>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

/// to-many 関連先に対する量化制約。
#[derive(Debug, Clone)]
pub enum QuantifierKind {
    Has,
    None,
}

/// One `.has(...)` / `.none(...)` chain call: whether some, or no, row of `related` matches
/// `conditions`. The rows are the ones linked to the entity the chain hangs off. No
/// conditions (`.none(Related)`) asks whether any linked row exists.
#[derive(Debug, Clone)]
pub struct RelatedQuantifier {
    pub kind: QuantifierKind,
    pub related: EntityKey,
    pub conditions: Vec<CrossEntityCondition>,
    /// `.role(role)` after the call: the key a link to `related` follows where several could.
    pub by: Vec<String>,

    /// Source site of the chain call.
    pub site: Option<crate::location::LocatedSpan>,
}

/// `invariant(...).when(...).has/none(...)` で宣言される集計制約。
#[derive(Debug, Clone)]
pub struct QuantifierConstraint {
    pub anchor: EntityKey,
    pub guards: Vec<CrossEntityCondition>,
    pub quantifier: RelatedQuantifier,
    /// `.along(Anchor, .., Related)`: the path whose keys chain the counted rows to the
    /// anchor's, in place of the one-hop link. `GlobalProduct` when the rule has none.
    pub along: CrossConstraintScope,

    /// `.label("...")`: the rule in the author's words, for the rule table and violations.
    pub label: Option<std::string::String>,
}

/// Which rule a `.when(Cond)` or a `.lemma()` belongs to: an index into the model's list of
/// that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleRef {
    Forbidden(usize),
    Invariant(usize),
    CrossForbidden(usize),
    CrossInvariant(usize),
    Quantifier(usize),
}

/// `.when(Cond)` on a `forbidden` or an `invariant`: the condition whose `holds` body joins
/// the rule's conditions, or its guards, once every predicate is read, since the body may
/// come later in the file than the rule (`analysis::constraint::resolve_rule_guards`).
#[derive(Debug, Clone)]
pub struct RuleGuard {
    pub rule: RuleRef,
    pub condition: super::ConditionKey,
    pub site: Option<crate::location::LocatedSpan>,
}

/// Atomic comparison over an entity column (equality or arithmetic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalAtom {
    /// The entity that owns `column`: the one written (`Order.status`), or the one entity
    /// with a column of that name.
    pub entity: EntityKey,
    pub column: std::string::String,
    pub op: super::comparison::CmpOpModel,
    pub rhs: TemporalRhs,
}

/// Right-hand side of a temporal atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalRhs {
    /// Enum / Bool / null / present (and Int via EffectValue::Int).
    Value(EffectValue),
    /// The values of `in` / `not in` (`Order.status in (open, paid)`).
    Values(Vec<EffectValue>),
    /// Integer literal (`stock < 5`).
    IntLit(i64),
    /// Column reference (`stock < selling` or `Item.stock < Item.selling`).
    Column {
        entity: EntityKey,
        column: std::string::String,
    },
}

/// Boolean expression inside a temporal property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalExpr {
    Atom(TemporalAtom),
    Not(Box<TemporalExpr>),
    And(Box<TemporalExpr>, Box<TemporalExpr>),
    Or(Box<TemporalExpr>, Box<TemporalExpr>),
}

/// Path property mapped to TLA+ temporal operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalFormula {
    /// `always(expr)` → `[]expr`
    Always(TemporalExpr),
    /// `eventually(expr)` → `<>expr`
    Eventually(TemporalExpr),
    /// `leads_to(p, q)` → `p ~> q`
    LeadsTo {
        antecedent: TemporalExpr,
        consequent: TemporalExpr,
    },
}

/// `property Id "label"` with a temporal formula body.
#[derive(Debug, Clone)]
pub struct TemporalProperty {
    pub id: std::string::String,
    pub label: std::string::String,
    pub formula: TemporalFormula,
    /// `.fair(UC, ...)`: the use cases assumed weakly fair per instance they act on.
    pub fair: Vec<UseCaseKey>,
    /// `.along(A, B, ..)`: the formula speaks about rows the keys of the path join, as a
    /// multi-entity `invariant` does; `GlobalProduct` without one.
    pub scope_semantics: CrossConstraintScope,
    /// `.global()`: the formula is meant for every combination of rows.
    pub global: bool,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}

impl TemporalFormula {
    /// The operands, left to right.
    pub fn exprs(&self) -> Vec<&TemporalExpr> {
        match self {
            TemporalFormula::Always(e) | TemporalFormula::Eventually(e) => vec![e],
            TemporalFormula::LeadsTo {
                antecedent,
                consequent,
            } => vec![antecedent, consequent],
        }
    }
}

impl TemporalExpr {
    /// The entities its atoms read, in order of first appearance, added to `out`.
    pub fn entities(&self, out: &mut Vec<EntityKey>) {
        let mut push = |ek: EntityKey| {
            if !out.contains(&ek) {
                out.push(ek);
            }
        };
        match self {
            TemporalExpr::Atom(a) => {
                push(a.entity);
                if let TemporalRhs::Column { entity, .. } = &a.rhs {
                    push(*entity);
                }
            }
            TemporalExpr::Not(x) => x.entities(out),
            TemporalExpr::And(x, y) | TemporalExpr::Or(x, y) => {
                x.entities(out);
                y.entities(out);
            }
        }
    }
}

impl TemporalProperty {
    /// The entities its formula reads, in order of first appearance.
    pub fn entities(&self) -> Vec<EntityKey> {
        let mut out = Vec::new();
        for e in self.formula.exprs() {
            e.entities(&mut out);
        }
        out
    }
}
