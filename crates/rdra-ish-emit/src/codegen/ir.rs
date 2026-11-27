//! The language-neutral domain model the printers print. Every choice (a name, a type, an
//! obligation) is made here once, so the three languages agree.

/// A column's value type before a printer names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    Int,
    String,
    Bool,
    Money,
    Decimal,
    Date,
    DateTime,
}

impl Scalar {
    /// The scalars whose representation a project picks: printed in the prelude.
    pub const PRELUDE: [Scalar; 4] = [
        Scalar::Money,
        Scalar::Decimal,
        Scalar::Date,
        Scalar::DateTime,
    ];

    /// The prelude type name.
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Int => "Int",
            Scalar::String => "String",
            Scalar::Bool => "Bool",
            Scalar::Money => "Money",
            Scalar::Decimal => "Decimal",
            Scalar::Date => "Date",
            Scalar::DateTime => "DateTime",
        }
    }

    /// What a prelude scalar holds by default, for its doc comment.
    pub fn default_doc(self) -> &'static str {
        match self {
            Scalar::Money => "A monetary amount as decimal text, such as \"1200.50\".",
            Scalar::Decimal => "A decimal number as text, such as \"0.3\".",
            Scalar::Date => "A date in ISO 8601, such as \"2026-11-07\".",
            Scalar::DateTime => {
                "A date and time in ISO 8601 with offset, such as \"2026-11-07T09:00:00+09:00\"."
            }
            Scalar::Int | Scalar::String | Scalar::Bool => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    Scalar(Scalar),
    /// A generated enum, by its type name.
    Enum(String),
    /// A generated id type, by its type name.
    Id(String),
}

/// The type of a function input: a value, or the rows a `.has` / `.none` ranges over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputType {
    Value(FieldType),
    /// A list of rows of a record or row type, by its type name.
    Rows(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumValue {
    /// The value as the model and the wire spell it.
    pub wire: String,
    /// The value as an identifier: PascalCase.
    pub ident: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub doc: Vec<String>,
    pub values: Vec<EnumValue>,
}

/// The id of an entity with a one-column primary key.
#[derive(Clone, Debug)]
pub struct IdDecl {
    pub name: String,
    pub entity: String,
    pub repr: Scalar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The column name, as the model and the wire spell it: the name in `<Entity>Row`.
    pub wire: String,
    /// The field's name in the domain types, snake_case: `wire`, except a column `kind` of a
    /// state type, which is `kind_` there, since `kind` is the discriminant
    /// (docs/design/algebraic-codegen.md, "Generated code style").
    pub name: String,
    /// The column name as an exported identifier: PascalCase.
    pub pascal: String,
    pub ty: FieldType,
    pub nullable: bool,
    pub doc: Vec<String>,
}

impl Field {
    /// The enum type of a field an [`Expr::EnumEq`] reads, which is always of an enum type.
    pub fn enum_ty(&self) -> &str {
        match &self.ty {
            FieldType::Enum(name) => name,
            other => unreachable!("an enum comparison reads a field of type {other:?}"),
        }
    }

    /// The domain name in PascalCase: `pascal`, with the `_` of an escaped name (`Kind_`).
    pub fn domain_pascal(&self) -> String {
        if self.name == self.wire {
            self.pascal.clone()
        } else {
            format!("{}_", self.pascal)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Entity,
    Dto,
}

#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub name: String,
    /// The entity or DTO id.
    pub id: String,
    pub kind: RecordKind,
    pub doc: Vec<String>,
    pub fields: Vec<Field>,
}

/// What the verification assumed and a type cannot enforce: the code that finishes the
/// output must keep it.
#[derive(Clone, Debug)]
pub struct Obligation {
    /// A name built from model identifiers only, so it does not move when the model gains an
    /// unrelated declaration: the invariant's name where `check` and `verify` check it
    /// (`Unique_Payment_Order`), else one of its own (`Ref_Product_category_id`).
    pub name: String,
    /// Whether `check` and `verify` check it, under `name`.
    pub checked: bool,
    pub text: String,
    pub site: Option<String>,
    /// What it is about, as data for a consumer that keeps it (a database mapper reading
    /// `codegen --format json`).
    pub subject: ObligationSubject,
}

/// The kind of an [`Obligation`] and the entity and columns it is about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObligationSubject {
    /// `reference`, `unique`, `min-children`, `max-children`, `on-delete`, `on-update`,
    /// `check`, `derived`, `invariant`, `rule-checks`, `transaction`, `run-marks`,
    /// `complete-rows`, or `not-met`.
    pub kind: &'static str,
    /// The entity whose rows it constrains, where it is one entity's.
    pub entity: Option<String>,
    /// Its columns of `entity`: the key columns of a reference, the columns a unique key
    /// spans, the column a check or a derivation is on.
    pub columns: Vec<String>,
    /// The entity a reference names: of `reference`, a `unique` through a key,
    /// `min-children`, `max-children`, `on-delete`, `on-update`.
    pub parent: Option<String>,
}

impl ObligationSubject {
    /// A subject of `kind` about no entity.
    pub fn of(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }
}

/// An entity with a lifecycle column: one variant per value of its tag column.
#[derive(Clone, Debug)]
pub struct SumDecl {
    /// The entity's type name: the union of the variants.
    pub name: String,
    pub entity: String,
    pub doc: Vec<String>,
    /// The tag column: a field whose type is the column's enum.
    pub tag: Field,
    pub variants: Vec<VariantDecl>,
    /// The flat row type `<Entity>Row`: every column as the data layer declares it, the
    /// shape a store or the wire holds.
    pub row: String,
    pub row_fields: Vec<Field>,
    /// The discriminant type `<Entity>Kind` (`<Entity>StateKind` when a column's enum has that
    /// name): one value per state, the domain's own, apart from the tag column.
    pub kind: String,
}

/// The error type of one use case, `<UseCase>Error`: the ways its functions, its dispatcher,
/// and a store's commit of its result fail.
#[derive(Clone, Debug)]
pub struct UseCaseErrorDecl {
    pub name: String,
    pub usecase: String,
    /// Whether a condition applied to it can reject a call.
    pub rejects: bool,
    /// Whether its arguments can break what the verified model keeps.
    pub invalid_argument: bool,
}

/// One state of a [`SumDecl`].
#[derive(Clone, Debug)]
pub struct VariantDecl {
    /// `<Entity><Value>`.
    pub name: String,
    /// The tag value of this state.
    pub value: EnumValue,
    pub doc: Vec<String>,
    /// Every column but the tag and the ones always null here. A nullable column a rule
    /// keeps present here is not nullable.
    pub fields: Vec<Field>,
    /// The nullable columns a rule keeps null in this state: no value to hold.
    pub always_null: Vec<Field>,
}

#[derive(Clone, Debug, Default)]
pub struct DomainModel {
    pub enums: Vec<EnumDecl>,
    pub ids: Vec<IdDecl>,
    pub records: Vec<RecordDecl>,
    pub sums: Vec<SumDecl>,
    pub events: Vec<EventDecl>,
    pub fns: Vec<FnDecl>,
    pub skipped: Vec<Skipped>,
    pub validators: Vec<ValidatorDecl>,
    pub dispatchers: Vec<DispatchDecl>,
    /// One per use case with functions, in the order of their first function.
    pub errors: Vec<UseCaseErrorDecl>,
    /// The names a language's printer derives, by key (`crate::codegen::derived_names`):
    /// the name as derived, or with `_` appended when a model type had it.
    pub derived: std::collections::BTreeMap<String, String>,
    /// One per actor that performs a use case with a contract, in id order (#337).
    pub capabilities: Vec<CapabilityDecl>,
    pub marks: Vec<MarkDecl>,
    /// Rules the reachable states agree with and the model does not state.
    pub candidates: Vec<String>,
    pub obligations: Vec<Obligation>,
    /// Every use case of the model, in model order, with the actions the transition system
    /// runs it as: empty for a model `check` rejects.
    pub usecases: Vec<UseCaseDecl>,
    /// The conformance port: its contracts are printed whenever it is set, and the harness
    /// when [`PortDecl::harness`] says so.
    pub port: Option<PortDecl>,
}

// ── Transition functions ─────────────────────────────────────────────────────

/// One function per action and per combination of the source states of its rows: every
/// type in it is concrete, so the three languages print it alike.
#[derive(Clone, Debug)]
pub struct FnDecl {
    /// PascalCase; the printers derive `camelCase` and `snake_case` from it.
    pub name: String,
    /// The use case it runs, and its source site.
    pub usecase: String,
    /// The transition system's action it prints.
    pub action: String,
    pub doc: Vec<String>,
    pub rows: Vec<RowParam>,
    pub inputs: Vec<FnInput>,
    /// Checks of the caller's arguments: a failure is a programming error, not a business
    /// outcome, and the function throws or panics.
    pub checks: Vec<ArgCheck>,
    /// The use case's condition guards: a failure returns `Rejected`.
    pub conditions: Vec<CondGuard>,
    pub outputs: Vec<RowOut>,
    /// The event types the use case raises.
    pub raises: Vec<RaisedEvent>,
    /// For a use case an event triggers: the event it runs on.
    pub event_in: Option<EventIn>,
    /// The rows a quantifier binds, numbered after `rows`: row `rows.len() + k` of an
    /// `Expr` is `elems[k]`.
    pub elems: Vec<RowParam>,
    /// The arms chosen among several from one value: of the state tag, or of a second
    /// lifecycle column.
    pub chosen: Vec<Chosen>,
    /// The result record's type name.
    pub result: String,
    /// For a step of a use case split into transactions: `(step, steps)`.
    pub step: Option<(u8, u8)>,
    /// The run marks of earlier steps the step continues, one per row they bound.
    pub marks_in: Vec<MarkIn>,
    /// The run marks the step leaves on its rows for the next step.
    pub marks_out: Vec<MarkOut>,
}

/// The target a function takes among several arms from one value.
#[derive(Clone, Debug)]
pub struct Chosen {
    /// The function row the arms move.
    pub row: usize,
    /// The second lifecycle column the arms move; `None` for the state tag.
    pub column: Option<String>,
    /// The enum type of the tag or the column.
    pub enum_ty: String,
    pub value: EnumValue,
}

impl FnDecl {
    /// The dispatcher's argument that takes the choice `c`: `<row>_to` for the state tag,
    /// `<row>_<column>_to` for a second lifecycle column.
    pub fn choice_name(&self, c: &Chosen) -> String {
        match &c.column {
            None => format!("{}_to", self.rows[c.row].name),
            Some(column) => format!("{}_{column}_to", self.rows[c.row].name),
        }
    }

    /// Row `r` of an `Expr`: an argument row, or a quantifier's element after them.
    pub fn row(&self, r: usize) -> &RowParam {
        self.rows
            .get(r)
            .unwrap_or_else(|| &self.elems[r - self.rows.len()])
    }

    /// What the row `o` writes must still be when the write commits: the state the function
    /// read it in, present, or absent for a row it creates.
    pub fn expected(&self, o: &RowOut) -> Expected {
        if let Some(row) = self.rows.iter().find(|r| r.param == Some(o.param)) {
            return match &row.state {
                Some((column, value)) => Expected::In {
                    column: column.clone(),
                    value: value.clone(),
                },
                None => Expected::Present,
            };
        }
        if self.marks_in.iter().any(|m| m.param == o.param) {
            return Expected::Present;
        }
        Expected::Absent
    }

    /// The run marks the step takes and does not advance, so the run ends on their rows: each
    /// is returned in the result field `ended_<mark>`, for the commit to delete
    /// (docs/design/algebraic-codegen.md, "Committing a result").
    pub fn marks_ended(&self) -> Vec<(&MarkIn, String)> {
        self.marks_in
            .iter()
            .filter(|m| !self.marks_out.iter().any(|o| o.param == m.param))
            .map(|m| (m, format!("ended_{}", m.name)))
            .collect()
    }
}

/// The source a written row must still have when the write commits, so a store can write it
/// conditionally (`UPDATE .. WHERE id = ? AND status = ?`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expected {
    /// A row the function creates: no row with its id exists.
    Absent,
    /// A row of a record type: it exists.
    Present,
    /// A row of a state type: its tag column holds the value the function read.
    In { column: String, value: String },
}

/// A run mark: the id of a row step `step` of a split use case wrote, which the next step
/// takes (docs/design/algebraic-codegen.md, "Use cases split into transactions").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkDecl {
    /// `<UseCase><Entity>Mark<step>`.
    pub name: String,
    pub usecase: String,
    pub entity: String,
    pub step: u8,
    /// The entity's id type.
    pub id_ty: String,
}

#[derive(Clone, Debug)]
pub struct MarkIn {
    /// snake_case.
    pub name: String,
    pub ty: String,
    /// The action param (an `Earlier` row) it stands for.
    pub param: usize,
}

#[derive(Clone, Debug)]
pub struct MarkOut {
    /// The result field, snake_case.
    pub name: String,
    pub ty: String,
    pub param: usize,
    /// The step it leaves the row at.
    pub step: u8,
    pub source: MarkSource,
}

/// Where a mark's id comes from.
#[derive(Clone, Debug)]
pub enum MarkSource {
    /// The id field of a row argument.
    Row { row: usize, field: Field },
    /// The id input of a created row.
    Input(usize),
    /// The id of a mark argument, advanced to this step.
    Mark(usize),
}

/// A row the function reads: a row it writes, or a parent it links a child to.
#[derive(Clone, Debug)]
pub struct RowParam {
    /// snake_case.
    pub name: String,
    /// The action param this row binds; none for a row a quantifier or a validator binds.
    pub param: Option<usize>,
    /// For a state type: the tag column and the source state.
    pub state: Option<(String, String)>,
    pub entity: String,
    /// The record or state type.
    pub ty: String,
    /// The type's fields, as it declares them.
    pub fields: Vec<Field>,
    /// The type's columns that are null in this state (absent from the Rust and Go struct).
    pub always_null: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FnInput {
    /// snake_case.
    pub name: String,
    pub ty: InputType,
    pub nullable: bool,
    pub doc: String,
}

#[derive(Clone, Debug)]
pub enum ArgCheck {
    /// The child row's key names the parent row.
    Link {
        child: usize,
        child_field: Field,
        parent: usize,
        parent_field: Field,
        message: String,
    },
    /// The row is the one the event argument was raised on.
    EventRow {
        row: usize,
        field: Field,
        event_field: String,
        message: String,
    },
    /// The row is the one the run mark argument `mark` names: a later step of a split use
    /// case that reads an earlier step's row takes it beside its mark.
    MarkRow {
        row: usize,
        field: Field,
        mark: usize,
        message: String,
    },
    /// The verified model keeps this column null here; a value means the rows broke a rule.
    MustBeNull {
        row: usize,
        field: Field,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct CondGuard {
    /// The condition's id.
    pub id: String,
    pub label: String,
    pub expr: Expr,
    pub site: Option<String>,
}

/// A condition body read on the function's rows. Atoms whose value the source state fixes
/// are folded to `True` or `False` when built.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    True,
    False,
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    /// A nullable field of a row is null.
    IsNull {
        row: usize,
        field: Field,
    },
    /// A field of an enum type holds `value`.
    EnumEq {
        row: usize,
        field: Field,
        value: EnumValue,
    },
    BoolEq {
        row: usize,
        field: Field,
        value: bool,
    },
    IntCmp {
        row: usize,
        field: Field,
        op: CmpOp,
        value: i64,
    },
    /// A nullable input is null: a condition on a value the caller chooses.
    InputIsNull {
        input: usize,
    },
    /// An enum input holds a value; a null input holds none (#359).
    InputEnumEq {
        input: usize,
        nullable: bool,
        enum_ty: String,
        value: EnumValue,
    },
    /// A Bool input holds a value; a null input holds none (#359).
    InputBoolEq {
        input: usize,
        nullable: bool,
        value: bool,
    },
    /// The child row's key names the parent row.
    KeyEq {
        child: usize,
        child_field: Field,
        parent: usize,
        parent_field: Field,
    },
    /// Some row of the input list `input`, bound as row `elem`, makes `body` hold.
    Exists {
        input: usize,
        elem: usize,
        body: Box<Expr>,
    },
    /// Every row of the input list `input`, bound as row `elem`, makes `body` hold.
    Forall {
        input: usize,
        elem: usize,
        body: Box<Expr>,
    },
    /// The rows of the input list `input`, bound as row `elem`, that make `body` hold,
    /// counted: `count op n`.
    Count {
        input: usize,
        elem: usize,
        body: Box<Expr>,
        op: CmpOp,
        n: u8,
    },
}

impl Expr {
    /// Folds the constant parts of an expression.
    pub(in crate::codegen) fn simplify(self) -> Expr {
        match self {
            Expr::Exists { input, elem, body } => match body.simplify() {
                Expr::False => Expr::False,
                body => Expr::Exists {
                    input,
                    elem,
                    body: Box::new(body),
                },
            },
            Expr::Forall { input, elem, body } => match body.simplify() {
                Expr::True => Expr::True,
                body => Expr::Forall {
                    input,
                    elem,
                    body: Box::new(body),
                },
            },
            Expr::Count {
                input,
                elem,
                body,
                op,
                n,
            } => Expr::Count {
                input,
                elem,
                body: Box::new(body.simplify()),
                op,
                n,
            },
            Expr::Not(inner) => match inner.simplify() {
                Expr::True => Expr::False,
                Expr::False => Expr::True,
                Expr::Not(x) => *x,
                other => Expr::Not(Box::new(other)),
            },
            Expr::And(items) => {
                let mut kept = Vec::new();
                for item in items.into_iter().map(Expr::simplify) {
                    match item {
                        Expr::True => {}
                        Expr::False => return Expr::False,
                        other => kept.push(other),
                    }
                }
                match kept.len() {
                    0 => Expr::True,
                    1 => kept.pop().expect("one item"),
                    _ => Expr::And(kept),
                }
            }
            Expr::Or(items) => {
                let mut kept = Vec::new();
                for item in items.into_iter().map(Expr::simplify) {
                    match item {
                        Expr::False => {}
                        Expr::True => return Expr::True,
                        other => kept.push(other),
                    }
                }
                match kept.len() {
                    0 => Expr::False,
                    1 => kept.pop().expect("one item"),
                    _ => Expr::Or(kept),
                }
            }
            other => other,
        }
    }

    /// The condition under which `self` fails, folded so that a negated comparison reads
    /// plainly.
    pub(in crate::codegen) fn fails(&self) -> Expr {
        Expr::Not(Box::new(self.clone())).simplify()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }

    /// Whether `a op b` holds.
    pub fn holds(self, a: i64, b: i64) -> bool {
        match self {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
        }
    }
}

/// A row the function returns.
#[derive(Clone, Debug)]
pub struct RowOut {
    /// The result field, snake_case.
    pub name: String,
    /// The action param this row is.
    pub param: usize,
    pub kind: OutKind,
}

impl RowOut {
    /// The result field that names what the row must still be when the write commits.
    pub fn expected_name(&self) -> String {
        format!("{}_expected", self.name)
    }
}

#[derive(Clone, Debug)]
pub enum OutKind {
    /// A written or created row: its type, its tag (for a state type), and every field.
    Row {
        ty: String,
        tag: Option<(String, EnumValue)>,
        fields: Vec<(Field, Assigned)>,
        /// The type's columns that are null in this state.
        always_null: Vec<String>,
    },
    /// A deleted row: the result names its id, when its entity has an id type.
    Deleted {
        row: usize,
        id: Option<(String, Field)>,
    },
}

/// What a field of an output row holds.
#[derive(Clone, Debug)]
pub enum Assigned {
    /// The source row's field. `adapt` turns its nullability into the target's.
    From {
        row: usize,
        field: Field,
        adapt: Adapt,
    },
    Lit {
        lit: Lit,
        wrap: bool,
    },
    Input {
        input: usize,
        wrap: bool,
    },
    /// The id of a parent row.
    ParentId {
        row: usize,
        field: Field,
        wrap: bool,
    },
    /// A second lifecycle column: the `to` of the arm whose `from` the row holds.
    Switch {
        row: usize,
        field: Field,
        enum_ty: String,
        arms: Vec<(EnumValue, EnumValue)>,
    },
    /// The id of the row the event argument was raised on; `param` is the action param.
    EventId {
        field: String,
        param: usize,
        wrap: bool,
    },
    /// The id of the row a mark argument names (a row an earlier step wrote).
    MarkId {
        mark: usize,
        wrap: bool,
    },
    Null,
    /// A `@derived` Bool column: its expression, read on what the function writes to the
    /// columns it names (an argument row's column it copies, or a value folded in).
    Derived {
        expr: Expr,
        wrap: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adapt {
    Same,
    /// Non-null into a nullable target.
    Wrap,
    /// Nullable into a non-null target: the verified model keeps it present here.
    Unwrap(String),
    /// The source state holds no such field (always null): the target is nullable.
    AbsentNull,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Enum { ty: String, value: EnumValue },
    Bool(bool),
    Int(i64),
}

/// An event type a use case raises.
#[derive(Clone, Debug)]
pub struct EventDecl {
    pub name: String,
    pub doc: Vec<String>,
    /// The ids of the rows the raise marks: a use case the event triggers runs only on them.
    pub fields: Vec<EventField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventField {
    pub entity: String,
    /// snake_case: `<entity>_id`.
    pub name: String,
    pub id_ty: String,
}

/// An event a function raises, with where each id of its payload comes from.
#[derive(Clone, Debug)]
pub struct RaisedEvent {
    pub ty: String,
    pub ids: Vec<(EventField, MarkSource)>,
}

/// The event a triggered use case's function takes.
#[derive(Clone, Debug)]
pub struct EventIn {
    /// snake_case.
    pub name: String,
    pub ty: String,
    /// The event's id.
    pub event: String,
}

/// A use case the generator printed no function for, and why.
#[derive(Clone, Debug)]
pub struct Skipped {
    pub action: String,
    /// The use case the action runs.
    pub usecase: String,
    pub reason: String,
}

/// The type a row that fits no state of its entity is refused with (`from_row` and the like).
pub const INVALID_ROW: &str = "InvalidRow";

/// The cases of every `<UseCase>Error`, each a type of its own every use case shares: a call
/// from states the use case does not run from (or a commit that found a row changed), a
/// condition that does not hold, and arguments that break what the verified model keeps.
pub const ERROR_CASES: [&str; 3] = ["NotEnabled", "Rejected", "InvalidArgument"];

/// A use case of the model.
#[derive(Clone, Debug)]
pub struct UseCaseDecl {
    pub id: String,
    pub label: String,
    /// Its declaration's site.
    pub site: Option<String>,
    /// The transition system's actions it runs as, in order: one, one per step of a split use
    /// case, or one per event of a triggered one. None for a use case that writes nothing.
    pub actions: Vec<String>,
    /// Its error type `<UseCase>Error` ([`UseCaseErrorDecl`]), when it has functions.
    pub error: Option<String>,
}

/// What the generated code offers for a use case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseCaseKind {
    /// Every action has functions.
    Functions,
    /// Some action has no function yet (the vocabulary is not printed, #335).
    Unprinted,
    /// It writes nothing: no action, no function, no entry point (decision 13 of
    /// docs/design/code-check.md).
    ReadOnly,
}

impl DomainModel {
    /// The name a printer derived under `key`, as claimed (`crate::codegen::derived_names`).
    pub fn derived(&self, key: &str) -> Option<&str> {
        self.derived.get(key).map(String::as_str)
    }

    /// The error type of the use case `usecase`.
    pub fn error_of(&self, usecase: &str) -> Option<&UseCaseErrorDecl> {
        self.errors.iter().find(|e| e.usecase == usecase)
    }

    /// The representation of the id type `id`; `Int` when the model declares no such id.
    pub fn id_repr(&self, id: &str) -> Scalar {
        self.ids
            .iter()
            .find(|i| i.name == id)
            .map_or(Scalar::Int, |i| i.repr)
    }

    /// The port, when the conformance harness is printed.
    pub fn harness(&self) -> Option<&PortDecl> {
        self.port.as_ref().filter(|p| p.harness)
    }

    /// The port, when it has a contract to probe.
    pub fn port_with_contracts(&self) -> Option<&PortDecl> {
        self.port.as_ref().filter(|p| !p.contracts.is_empty())
    }

    /// The name of the error type of `usecase`, which has a function or an entry point: every
    /// such use case has one.
    pub fn error_name(&self, usecase: &str) -> &str {
        &self
            .error_of(usecase)
            .expect("a use case with a function or an entry point has an error type")
            .name
    }

    /// Whether the arguments of `f` can break what the verified model keeps: an argument
    /// check, a nullable column read as present, or a second lifecycle column in no `from`
    /// state of its arms.
    pub fn invalid_argument_possible(&self, f: &FnDecl) -> bool {
        if !f.checks.is_empty() {
            return true;
        }
        f.outputs.iter().any(|o| match &o.kind {
            OutKind::Row { fields, .. } => fields.iter().any(|(_, a)| match a {
                Assigned::From {
                    adapt: Adapt::Unwrap(_),
                    ..
                } => true,
                Assigned::Switch { enum_ty, arms, .. } => {
                    arms.len()
                        < self
                            .enums
                            .iter()
                            .find(|e| e.name == *enum_ty)
                            .map_or(0, |e| e.values.len())
                }
                _ => false,
            }),
            OutKind::Deleted { .. } => false,
        })
    }

    /// What the code offers for the use case `uc`.
    pub fn usecase_kind(&self, uc: &UseCaseDecl) -> UseCaseKind {
        if uc.actions.is_empty() {
            UseCaseKind::ReadOnly
        } else if self.skipped.iter().any(|s| s.usecase == uc.id) {
            UseCaseKind::Unprinted
        } else {
            UseCaseKind::Functions
        }
    }
}

// ── Rule validators ──────────────────────────────────────────────────────────

/// The entity-local rules of one record or state type, as checks on one row.
#[derive(Clone, Debug)]
pub struct ValidatorDecl {
    /// The record or state type it validates.
    pub ty: String,
    /// The entity id.
    pub entity: String,
    /// The row, as `Expr` reads it (row 0).
    pub row: RowParam,
    pub rules: Vec<RuleCheck>,
}

/// One rule: the row keeps it when `expr` holds.
#[derive(Clone, Debug)]
pub struct RuleCheck {
    /// The invariant's name, as `check` and `verify` report it.
    pub name: String,
    pub text: String,
    pub expr: Expr,
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

/// One function per use case over its rows' entity types: it picks the transition function
/// of the rows' states, or reports that the use case does not run from them.
#[derive(Clone, Debug)]
pub struct DispatchDecl {
    /// `Run<Action>`.
    pub name: String,
    pub usecase: String,
    /// The outcome type: one case per transition function.
    pub outcome: String,
    /// `(name, entity type)` per row, in the transition functions' order.
    pub rows: Vec<(String, String)>,
    /// `(name, type)` per run mark argument, after the rows.
    pub marks: Vec<(String, String)>,
    /// `(name, tag enum)` per choice argument: the state the actor picks, after the inputs.
    pub choices: Vec<(String, String)>,
    /// Every input any of the functions takes, by name.
    pub inputs: Vec<FnInput>,
    pub cases: Vec<DispatchCase>,
    /// Whether some case can be rejected by a condition.
    pub rejects: bool,
}

// ── Conformance ──────────────────────────────────────────────────────────────

/// The port a finished service implements so the conformance harness can drive it, and what
/// the harness reads of the store (docs/design/algebraic-codegen.md, "The conformance
/// harness"). Printed only on request.
#[derive(Clone, Debug, Default)]
pub struct PortDecl {
    /// Every entity of the transition system: the rows a snapshot holds.
    pub entities: Vec<PortEntity>,
    /// Every run mark the system keeps: one per split use case and entity a step before the
    /// last writes.
    pub runs: Vec<PortRun>,
    /// Every event the system remembers being raised on a row.
    pub events: Vec<PortEvent>,
    /// One per action the generator printed functions for.
    pub methods: Vec<PortMethod>,
    /// One contract per use case with methods, in the order of its first method:
    /// `ConformancePort` is their composition, with `seed` and `snapshot`.
    pub contracts: Vec<ContractDecl>,
    /// What the store holds that the harness does not compare, for the header.
    pub unchecked: Vec<String>,
    /// Whether the conformance harness is printed (`--conformance`): the snapshot,
    /// `ConformancePort`, and `conform`.
    pub harness: bool,
}

/// The contract of one use case (docs/design/code-check.md, decision 6): a trait / interface
/// with the prepare and commit methods of its actions, which the service implements and the
/// probe names. One per use case, so a compiler error on it names the use case.
#[derive(Clone, Debug)]
pub struct ContractDecl {
    /// `<UseCase>UseCase`, or `<UseCase>Contract` when a model type has that name.
    pub name: String,
    /// The use case's id.
    pub usecase: String,
    pub label: String,
    /// Where the use case is declared, `path:line:col`.
    pub site: Option<String>,
    /// The use case's error type, `<UseCase>Error`.
    pub error: String,
    /// Its methods: indices into [`PortDecl::methods`], one per action with functions.
    pub methods: Vec<usize>,
    /// One entry point per action the generator printed no function for (#335): the service
    /// runs it whole, so the probe still requires it.
    pub entries: Vec<EntryDecl>,
    /// Who may run it (#337): `None` for a use case no actor performs (an event or a timer
    /// starts it, or the model names no actor).
    pub caller: Option<CallerDecl>,
}

/// The capability of one actor (#337): a value the code that authenticated the actor makes,
/// and that every prepare and entry point of a use case the actor performs takes.
#[derive(Clone, Debug)]
pub struct CapabilityDecl {
    /// The actor's id.
    pub actor: String,
    pub label: String,
    /// `<Actor>Capability`, or `<Actor>Cap` when a model type has that name.
    pub name: String,
}

/// Who may run one use case: one of the capabilities of the actors that perform it.
#[derive(Clone, Debug)]
pub struct CallerDecl {
    /// `<UseCase>Caller`, or `<UseCase>UseCaseCaller`.
    pub name: String,
    /// Indices into [`DomainModel::capabilities`], in actor id order.
    pub actors: Vec<usize>,
}

/// The entry point of an action with no printed function: `run_<action>` / `Run<Action>`
/// by the ids of its rows, returning the use case's error when it does not run.
#[derive(Clone, Debug)]
pub struct EntryDecl {
    /// The transition system's action.
    pub action: String,
    /// PascalCase.
    pub name: String,
    /// `(name, id type)` per row of the action: an existing row, or the id of a created one.
    pub rows: Vec<(String, String)>,
}

/// The rows of one entity in a snapshot.
#[derive(Clone, Debug)]
pub struct PortEntity {
    pub entity: String,
    /// The snapshot field, snake_case.
    pub field: String,
    /// The row type: `<Entity>Row` of a state type, or the entity's record.
    pub ty: String,
    /// The primary key column.
    pub id: Field,
    /// Every column of the row type, in order.
    pub columns: Vec<Field>,
    /// The columns the transition system tracks, in column order, and how.
    pub tracked: Vec<(Field, Track)>,
}

/// How the harness abstracts a tracked column to the value the transition system holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    /// The value itself: an enum's name, a bool, an integer, a key's id; `~` for null.
    Exact,
    /// Only whether it is set: `+`, or `~` for null.
    Presence,
}

/// The run marks of one split use case on one entity, as a store keeps them.
#[derive(Clone, Debug)]
pub struct PortRun {
    /// `<UseCase><Entity>Run`.
    pub name: String,
    /// The snapshot field, snake_case.
    pub field: String,
    pub usecase: String,
    pub entity: String,
    pub id_ty: String,
}

/// The events of one type a store remembers being raised.
#[derive(Clone, Debug)]
pub struct PortEvent {
    pub ty: String,
    /// The snapshot field, snake_case.
    pub field: String,
    /// The event's id fields; `true` where the system remembers the raise on that row.
    pub ids: Vec<(EventField, bool)>,
}

/// One action as the service runs it: it loads the rows by id and stores what the core returns.
#[derive(Clone, Debug)]
pub struct PortMethod {
    /// The transition system's action, which the traces name.
    pub action: String,
    /// PascalCase: `run_<snake>` in Rust, `Run<Pascal>` in Go.
    pub name: String,
    pub usecase: String,
    /// Per row the functions take: its id type.
    pub rows: Vec<Arg>,
    /// Per run mark the functions take: the id type of the row it marks.
    pub marks: Vec<Arg>,
    /// The event a triggered use case takes: its event type.
    pub event: Option<Arg>,
    /// Every input any of the functions takes but the lists of related rows, which the
    /// service loads.
    pub inputs: Vec<PortInput>,
    /// Per choice among arms: its tag enum.
    pub choices: Vec<Arg>,
    /// What the read returns and the write takes: the dispatcher's outcome, or the one
    /// function's result.
    pub write: String,
}

/// A value input of a port method: the lists of related rows are the service's to load.
#[derive(Clone, Debug)]
pub struct PortInput {
    /// snake_case, as the functions name it.
    pub name: String,
    pub ty: FieldType,
    pub nullable: bool,
}

/// A port method's argument whose type the IR names.
#[derive(Clone, Debug)]
pub struct Arg {
    pub name: String,
    pub ty: String,
}

/// One argument of a port method.
#[derive(Clone, Copy, Debug)]
pub enum PortArg<'a> {
    /// A row's or a run mark's id; `ty` is the id type.
    Id(&'a Arg),
    /// The event a triggered use case takes; `ty` is the event type.
    Event(&'a Arg),
    Input(&'a PortInput),
    /// A choice among arms; `ty` is the tag enum.
    Choice(&'a Arg),
}

impl PortArg<'_> {
    /// The argument's name, as the IR spells it.
    pub fn name(&self) -> &str {
        match self {
            PortArg::Id(a) | PortArg::Event(a) | PortArg::Choice(a) => &a.name,
            PortArg::Input(i) => &i.name,
        }
    }
}

impl PortMethod {
    /// The arguments in the order every language passes them: rows, run marks, the event,
    /// inputs, choices.
    pub fn args(&self) -> impl Iterator<Item = PortArg<'_>> {
        self.rows
            .iter()
            .chain(&self.marks)
            .map(PortArg::Id)
            .chain(self.event.iter().map(PortArg::Event))
            .chain(self.inputs.iter().map(PortArg::Input))
            .chain(self.choices.iter().map(PortArg::Choice))
    }
}

#[derive(Clone, Debug)]
pub struct DispatchCase {
    /// The transition function, its result type, and the outcome case (PascalCase).
    pub function: String,
    pub result: String,
    pub case: String,
    pub rejects: bool,
    /// Per row: `None` for a record, or the state's (type, tag column, value).
    pub states: Vec<Option<(String, String, EnumValue)>>,
    /// The indices into the dispatcher's inputs the function takes, in its order.
    pub inputs: Vec<usize>,
    /// The target state a choice argument must name: `(index into choices, target)`.
    pub choice: Vec<(usize, EnumValue)>,
}
