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
    /// The column name, as the model and the wire spell it.
    pub wire: String,
    /// The column name as an exported identifier: PascalCase.
    pub pascal: String,
    pub ty: FieldType,
    pub nullable: bool,
    pub doc: Vec<String>,
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
    /// `O1`, `O2`, ...
    pub id: String,
    /// The invariant `check` and `verify` name it by, when they check it.
    pub invariant: Option<String>,
    pub text: String,
    pub site: Option<String>,
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
    pub marks: Vec<MarkDecl>,
    /// Rules the reachable states agree with and the model does not state.
    pub candidates: Vec<String>,
    pub obligations: Vec<Obligation>,
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
    /// The arms chosen among several from one state: `(row, tag enum, target)`.
    pub chosen: Vec<(usize, String, EnumValue)>,
    /// The result record's type name.
    pub result: String,
    /// For a step of a use case split into transactions: `(step, steps)`.
    pub step: Option<(u8, u8)>,
    /// The run marks of earlier steps the step continues, one per row they bound.
    pub marks_in: Vec<MarkIn>,
    /// The run marks the step leaves on its rows for the next step.
    pub marks_out: Vec<MarkOut>,
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
    /// The action param this row binds.
    pub param: usize,
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
    pub ty: FieldType,
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
    EnumEq {
        row: usize,
        field: Field,
        enum_ty: String,
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
    /// The id of the row a mark argument names (a row an earlier step wrote).
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
    MarkId {
        mark: usize,
        wrap: bool,
    },
    Null,
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
}

/// A use case the generator printed no function for, and why.
#[derive(Clone, Debug)]
pub struct Skipped {
    pub action: String,
    pub reason: String,
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
