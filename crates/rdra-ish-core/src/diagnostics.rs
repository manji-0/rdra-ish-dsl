use thiserror::Error;

#[derive(Debug, Error)]
pub enum RdraError {
    #[error("undefined symbol: {id}")]
    UndefinedSymbol { id: String },

    #[error("ambiguous reference: '{id}' matches multiple kinds ({kinds})\n  hint: use a kind-qualified reference, e.g. `usecase::{id}`")]
    AmbiguousReference { id: String, kinds: String },

    #[error("type mismatch in predicate '{pred}': argument '{id}' has kind {actual}, expected {expected}\n  hint: check that you are passing the right element type")]
    TypeMismatch {
        pred: String,
        id: String,
        actual: String,
        expected: String,
    },

    #[error("duplicate definition: '{id}' is already defined with the same kind\n  hint: each element id must be unique per kind across all imported files")]
    DuplicateDefinition { id: String },

    #[error("N:M relation between '{from}' and '{to}': direct N:M relations are not supported\n  hint: create an intermediate entity, e.g.:\n    entity {from}{to} \"..\" {{ id: Int @pk }}\n    relate({from}{to}, {from}, \"N:1\")\n    relate({from}{to}, {to}, \"N:1\")")]
    NMRelation { from: String, to: String },

    #[error("FK conflict: entity '{entity}' already has a column '{col}' that conflicts with auto-generated FK")]
    FkConflict { entity: String, col: String },

    #[error("missing @pk: entity '{entity}' used as FK target has no @pk column\n  hint: add `id: Int @pk` (or another @pk column) to entity '{entity}'")]
    MissingPk { entity: String },

    #[error("IO error reading '{path}': {msg}")]
    IoError { path: String, msg: String },

    #[error("circular import detected among files: {}", files.join(", "))]
    CircularImport { files: Vec<String> },

    #[error("usecase '{usecase}' writes '{entity}' with no FK link to its other writes\n  hint: this is inferred as a separate transaction; if it must be atomic with the others, model the operation through an API boundary")]
    SeparateTxInferred { usecase: String, entity: String },

    // ── sets(...) カラム効果述語の診断 ──────────────────────────────────────
    #[error("unknown column: entity '{entity}' has no column '{col}'\n  hint: check the column name in the entity definition")]
    UnknownColumn { entity: String, col: String },

    #[error("invalid enum value: column '{col}' has no variant '{value}'\n  hint: declared variants are: {allowed}")]
    InvalidEnumVariant {
        col: String,
        value: String,
        allowed: String,
    },

    #[error("invalid bool value: column '{col}' set to '{value}'; expected `true` or `false`")]
    InvalidBoolValue { col: String, value: String },

    #[error("null/present effect on non-nullable column '{col}'\n  hint: only @null columns accept `null`/`present` or PostgreSQL type tokens")]
    NullOnNonNullable { col: String },

    #[error("effect on non-state column '{col}' (type {col_type}): only Enum, Bool, @null, or PostgreSQL-typed columns define state\n  hint: use `null`, `present`, an enum variant, `true`/`false`, or a known PostgreSQL type name (e.g. `jsonb`, `timestamptz`)")]
    EffectOnNonStateColumn { col: String, col_type: String },

    #[error("unknown effect value '{value}' for column '{col}': not an enum variant, bool, null/present, or known PostgreSQL type\n  hint: see the allowed value vocabulary in the docs")]
    UnknownEffectValue { col: String, value: String },

    // ── 比較式の診断 ─────────────────────────────────────────────────────────
    #[error("comparison left-hand side must be a column reference, got a literal or `now`\n  hint: write `col < other_col` or `col < 0`")]
    ComparisonLhsMustBeColumn,

    #[error("order comparison operator '{op}' cannot be applied to column '{col}' of type {col_type}\n  hint: only Int, Money, Decimal, Date, DateTime columns support <, >, <=, >=; use == or != for other types")]
    ComparisonOpNotOrdered {
        col: String,
        col_type: String,
        op: String,
    },

    #[error("comparison type mismatch: column '{lhs}' ({lhs_type}) and '{rhs}' ({rhs_type}) are not comparable\n  hint: both sides must be in the same type category (numeric or temporal)")]
    ComparisonTypeMismatch {
        lhs: String,
        lhs_type: String,
        rhs: String,
        rhs_type: String,
    },

    #[error("right-hand side column '{col}' not found in entity '{entity}'\n  hint: check the column name in the entity definition")]
    ComparisonRhsColumnUnknown { entity: String, col: String },

    #[error(
        "comparison with `now` requires a Date or DateTime column, but '{col}' has type {col_type}"
    )]
    ComparisonNowRequiresTemporal { col: String, col_type: String },

    #[error("invalid integer literal '{lit}' in comparison right-hand side")]
    ComparisonInvalidIntLit { lit: String },

    // ── イベント整合性診断 ────────────────────────────────────────────────────
    #[error("event '{event}' is never raised by any use case\n  hint: add `raises(usecase::Foo, event::{event})` in the relevant BUC file")]
    EventNeverRaised { event: String },

    #[error("event '{event}' is raised but has no transitions and triggers no use case\n  hint: add `transitions(event::{event}, From, To)` or `triggers(event::{event}, someUsecase)` to connect this event")]
    EventNeverConsumed { event: String },

    #[error("event '{event}' triggers use case '{usecase}' which belongs to no BUC\n  hint: add `contains(someBuc, usecase::{usecase})` to include the triggered use case in a BUC")]
    TriggeredUseCaseUnreachable { event: String, usecase: String },

    // ── API 整合性診断 ────────────────────────────────────────────────────────
    #[error("api '{api}' is declared but never invoked by any use case\n  hint: add `invokes(usecase::Foo, api::{api})` in the relevant BUC file")]
    ApiNeverInvoked { api: String },

    #[error("api '{api}' is invoked but operates no entity\n  hint: add `creates/updates/reads(api::{api}, SomeEntity)` to give the API work to do")]
    ApiInvokedButNoEntity { api: String },
}

#[derive(Debug)]
pub struct Diagnostic {
    pub error: RdraError,
    pub is_warning: bool,
}

impl Diagnostic {
    pub fn error(e: RdraError) -> Self {
        Self {
            error: e,
            is_warning: false,
        }
    }

    pub fn warning(e: RdraError) -> Self {
        Self {
            error: e,
            is_warning: true,
        }
    }
}
