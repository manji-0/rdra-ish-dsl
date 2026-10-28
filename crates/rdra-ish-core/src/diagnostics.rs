use thiserror::Error;

#[derive(Debug, Error)]
pub enum RdraError {
    #[error("api '{api}' reuses route {method} {path} already used by api '{other}'\n  hint: give each API a unique method+path pair")]
    DuplicateApiRoute {
        api: String,
        other: String,
        method: String,
        path: String,
    },

    #[error("unknown import '{name}' from module '{module}'\n  hint: check the name is declared in that module")]
    UnknownImportName { name: String, module: String },

    #[error("unknown module '{module}'\n  hint: declare `module {module}` in an included .rdra file, or fix the import path")]
    UnknownImportModule { module: String },

    #[error("duplicate module '{module}'\n  hint: each module path must be declared in exactly one file")]
    DuplicateModule { module: String },

    #[error("undefined symbol: {id}")]
    UndefinedSymbol { id: String },

    #[error("ambiguous reference: '{id}' matches multiple declarations ({kinds})\n  hint: if kinds differ, qualify with `kind::{id}`; if the same kind appears in multiple modules, use `import M as a` then `a.{id}`")]
    AmbiguousReference { id: String, kinds: String },

    #[error("duplicate import alias '{alias}'\n  hint: each `import … as alias` name must be unique in the file; rename one alias")]
    DuplicateAlias { alias: String },

    #[error("type mismatch in predicate '{pred}': argument '{id}' has kind {actual}, expected {expected}\n  hint: {hint}", hint = type_mismatch_hint(pred, id, actual))]
    TypeMismatch {
        pred: String,
        id: String,
        actual: String,
        expected: String,
    },

    #[error("duplicate definition: '{id}' is already defined with the same kind\n  hint: each element id must be unique per kind across all imported files")]
    DuplicateDefinition { id: String },

    #[error("requirement metadata is only valid on requirement '{id}' declarations\n  hint: move priority/source/stakeholder/owner/acceptance/status/risk/rationale clauses to a `requirement` declaration")]
    RequirementMetadataOnNonRequirement { id: String },

    #[error("ADR metadata is only valid on adr '{id}' declarations\n  hint: move adr_status/context/decision/consequence/accepted/rejected/reason clauses to an `adr` declaration")]
    AdrMetadataOnNonAdr { id: String },

    #[error("api metadata is only valid on api '{id}' declarations\n  hint: move method/path/idempotency/mode/auth clauses to an `api` declaration")]
    ApiMetadataOnNonApi { id: String },

    #[error("non-functional metadata is only valid on nfr or constraint '{id}' declarations\n  hint: move metric/target/window/slo/availability/resilience/audit/logging/retention/privacy clauses to an `nfr` or `constraint` declaration")]
    NfrMetadataOnInvalidKind { id: String },

    #[error("field metadata is only valid on field '{id}' declarations\n  hint: move access/required/source/input/derived clauses to a `field` declaration")]
    FieldMetadataOnNonField { id: String },

    #[error("usecase metadata is only valid on usecase '{id}' declarations\n  hint: move precondition/postcondition/guard/alternative/error clauses to a `usecase` declaration")]
    UseCaseMetadataOnNonUseCase { id: String },

    #[error("N:M relation between '{from}' and '{to}' requires .via(Junction)\n  hint: declare a junction entity and attach it, e.g.:\n    entity {from}{to} \"..\" {{ id: Int @pk }}\n    relate({from}, {to}, N:M).via({from}{to})\n  equivalently:\n    relate({from}{to}, {from}, N:1)\n    relate({from}{to}, {to}, N:1)")]
    NMRelation { from: String, to: String },

    #[error(".via(...) is only valid on relate(..., N:M)\n  hint: use `relate(A, B, N:M).via(Junction)` or drop `.via` for 1:1 / 1:N / N:1")]
    ViaOnlyForNM,

    #[error(".via(...) must name a single entity junction\n  hint: write `relate(A, B, N:M).via(JunctionEntity)`")]
    ViaRequiresEntity,

    #[error(".via(...) junction '{via}' must be distinct from relate endpoints '{from}' and '{to}'\n  hint: declare a separate junction entity, e.g. `entity {from}{to} \"..\" {{ id: Int @pk }}` then `.via({from}{to})`")]
    ViaMustBeDistinct {
        from: String,
        to: String,
        via: String,
    },

    #[error("self N:M relate('{entity}', '{entity}', N:M) is not supported\n  hint: use an explicit junction entity that is not '{entity}'")]
    SelfNMRelation { entity: String },

    #[error("junction '{junction}' is used as .via for multiple N:M pairs ({pairs})\n  hint: give each N:M relation its own junction entity so composite unique and FK meaning stay unambiguous")]
    SharedJunctionVia { junction: String, pairs: String },

    #[error("FK conflict: entity '{entity}' already has a column '{col}' that conflicts with auto-generated FK\n  hint: name the key with `.role(...)` so it generates other columns, or adopt the column with `.key({col})`")]
    FkConflict { entity: String, col: String },

    #[error("entity '{child}' has two relate keys with role '{role}'\n  hint: add `.role(...)` to one of them so each key gets its own columns and owner")]
    DuplicateRelateRole { child: String, role: String },

    #[error(".along(...) cannot tell which key joins '{a}' and '{b}': roles {roles}\n  hint: name one with `.along(...).by({first})`")]
    AmbiguousOwnerLink {
        a: String,
        b: String,
        roles: String,
        first: String,
    },

    #[error(".by({role}) names no key on .along({along})\n  hint: use the role of a key between neighbours on the path: {roles}")]
    UnknownOwnerRole {
        role: String,
        along: String,
        roles: String,
    },

    #[error(".{method}(...) on relate(..., N:M) has no key to shape: an N:M holds no key of its own\n  hint: write the junction's two keys as `relate(Junction, A, N:1).{method}(...)` and `relate(Junction, B, N:1)` instead of `.via(Junction)`")]
    KeyOptionOnManyToMany { method: String },

    #[error(".by(...) picks a key on an .along(...) path, and this constraint has none\n  hint: add `.along(A, B)` before `.by(...)`, or drop `.by(...)`")]
    ByWithoutAlong,

    #[error("missing @pk: entity '{entity}' used as FK target has no @pk column\n  hint: add `id: Int @pk` (or another @pk column) to entity '{entity}'")]
    MissingPk { entity: String },

    #[error(".key(...) on relate({child}, {parent}, ..) names {got} column(s) but '{parent}' has a {want}-column key\n  hint: list one '{child}' column per '{parent}' key column, in key order")]
    KeyArity {
        child: String,
        parent: String,
        got: usize,
        want: usize,
    },

    #[error(".key(...) names '{column}', which entity '{child}' does not declare\n  hint: declare the column on '{child}', or drop `.key(...)` to generate it")]
    KeyColumnUndefined { child: String, column: String },

    #[error(".key(...) column '{child}.{column}' is {got} but the parent key column '{parent_column}' is {want}\n  hint: give '{column}' the type {want}")]
    KeyTypeMismatch {
        child: String,
        column: String,
        got: String,
        parent_column: String,
        want: String,
    },

    #[error(".key(...) column '{child}.{column}' already holds another relate's key\n  hint: give each relate its own key columns")]
    KeyColumnShared { child: String, column: String },

    #[error("cardinality range '{range}' is out of range: the low end is 0 or 1, the high end 1 or *\n  hint: write `0..*`, `1..*`, `0..1` or `1..1`")]
    MinCardOutOfRange { range: String },

    #[error("'{card}' puts a range on the parent side: a range counts the children per parent\n  hint: write that side as `1`, and add `.optional()` when a child may have no parent")]
    MinCardOnParentSide { card: String },

    #[error("'{card}' puts a range on an N:M, which holds no key of its own\n  hint: give the junction's keys the range, `relate(Junction, A, 1..*:1)`, instead of `.via(Junction)`")]
    MinCardOnManyToMany { card: String },

    #[error("unknown referential action '{got}' in .{method}(...)\n  hint: use one of cascade, restrict, set_null, set_default, no_action")]
    UnknownRefAction { method: String, got: String },

    #[error("IO error reading '{path}': {msg}")]
    IoError { path: String, msg: String },

    #[error("syntax error in '{path}': {msg}")]
    SyntaxError { path: String, msg: String },

    #[error("circular import detected among files: {}", files.join(", "))]
    CircularImport { files: Vec<String> },

    #[error("usecase '{usecase}' writes '{entity}' in its own transaction after the FK-grouped writes\n  hint: it has no FK link to them; if it must be atomic with them, model the operation through an API boundary")]
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

    #[error("column '{col}' of '{entity}' is typed by '{name}', which names no variation\n  hint: declare `variation {name} \"Label\" {{ value ... }}` and import it, or use an inline `Enum(...)`")]
    UnknownColumnType {
        entity: String,
        col: String,
        name: String,
    },

    #[error("invalid variation value: '{value}' is not a value of variation '{variation}'\n  hint: declared values are: {allowed}")]
    InvalidVariationValue {
        variation: String,
        value: String,
        allowed: String,
    },

    #[error("condition '{condition}' declares both `combines` and `holds`\n  hint: with a `holds` body the variation and state-model links are derived from the columns it names; delete the `combines` lines, or delete `holds` and keep `combines` for traceability only")]
    ConditionCombinesWithBody { condition: String },

    #[error("condition '{condition}' already has a `holds` body\n  hint: list every comparison in one `holds(...)`; the comparisons are an implicit conjunction")]
    ConditionSecondBody { condition: String },

    #[error("`combines` links condition '{condition}' to '{entity}.{col}', which is not a lifecycle column\n  hint: a state model is a column named by `transitions`; to link a variation write `combines({condition}, SomeVariation)` instead")]
    ConditionCombinesAttribute {
        condition: String,
        entity: String,
        col: String,
    },

    #[error("`combines` target '{target}' names both variation '{variation}' and a column of entity '{entity}'\n  hint: write `variation::{variation}` to mean the variation, or rename the variation or the column")]
    AmbiguousCombinesTarget {
        target: String,
        variation: String,
        entity: String,
    },

    #[error("transitions first argument must be `Entity.column`, got '{got}'\n  hint: write `transitions(Order.status, CapturePayment, pending -> paid)`")]
    InvalidTransitionsColumnRef { got: String },

    #[error("transitions column '{col}' on entity '{entity}' must be an inline Enum column\n  hint: lifecycle transitions bind an inline Enum status/lifecycle column; a column typed by a variation is an attribute and has no lifecycle")]
    TransitionsColumnNotEnum { entity: String, col: String },

    #[error("invalid bool value: column '{col}' set to '{value}'; expected `true` or `false`")]
    InvalidBoolValue { col: String, value: String },

    #[error("null/present effect on non-nullable column '{col}'\n  hint: only @null columns accept `null`/`present`")]
    NullOnNonNullable { col: String },

    #[error("effect on non-state column '{col}' (type {col_type}): only Enum, Bool, or @null columns define state\n  hint: use `null`, `present`, an enum variant, `true`/`false`, or an integer literal for Int/Money/Decimal")]
    EffectOnNonStateColumn { col: String, col_type: String },

    #[error("PostgreSQL type name '{name}' is not valid in sets(...); use `present` for a non-null nullable value\n  hint: declare display/storage types with entity annotations or DBML export inference")]
    PgTypeNameInSets { name: String },

    #[error("sets(..., {col} == {value}, false) is invalid: the fourth argument drives comparison propositions (e.g. `stock < selling`), not value equality\n  hint: omit `, false` and set a different value with `sets(..., {col} == other)`, or use a comparison proposition")]
    SetsFalseOnEquals { col: String, value: String },

    #[error("entity '{entity}' already has transitions on column '{existing}', ignoring additional column '{column}'\n  hint: one Enum lifecycle column per entity is supported; move the other lifecycle to a separate entity or drop the extra `transitions`")]
    MultipleLifecycleColumns {
        entity: String,
        existing: String,
        column: String,
    },

    #[error("unknown effect value '{value}' for column '{col}': not an enum variant, bool, null/present, or integer\n  hint: see the allowed value vocabulary in the docs")]
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

    #[error("cross-entity constraint column '{column}' needs an entity qualifier\n  hint: write `{example}` so the target entity is explicit")]
    CrossConstraintColumnNeedsEntity { column: String, example: String },

    // ── 権限整合性診断 ─────────────────────────────────────────────────────────
    #[error("usecase '{usecase}' requires permission '{permission}', but no actor performs the use case or its containing BUC\n  hint: add `performs(SomeActor, {usecase})`, add `performs(SomeActor, SomeBuc)` where `contains(SomeBuc, {usecase})`, or remove the permission requirement if this is a system-triggered step")]
    UseCasePermissionNoActor { usecase: String, permission: String },

    #[error("usecase '{usecase}' requires permission '{permission}', but none of its actors ({actors}) has that permission\n  hint: add `has_permission(SomeActor, {permission})` for an actor that performs this use case path")]
    UseCasePermissionNotHeld {
        usecase: String,
        permission: String,
        actors: String,
    },

    #[error("api '{api}' requires permission '{permission}', but invoking usecase '{usecase}' has no actor path\n  hint: add a performer for the use case or containing BUC, or remove the API permission requirement if the invocation is system-triggered")]
    ApiPermissionNoActor {
        api: String,
        permission: String,
        usecase: String,
    },

    #[error("api '{api}' requires permission '{permission}', but invoking usecase '{usecase}' is performed by actors ({actors}) without that permission\n  hint: add `has_permission(SomeActor, {permission})` for an actor on this invocation path or split the API boundary")]
    ApiPermissionNotHeld {
        api: String,
        permission: String,
        usecase: String,
        actors: String,
    },

    #[error("actor '{actor}' is missing permission '{permission}' required by {required_by}\n  hint: add `has_permission({actor}, {permission})`, narrow the actor's `performs` path, or relax the UC/API requirement if it is too strict")]
    ActorPermissionMissing {
        actor: String,
        permission: String,
        required_by: String,
    },

    #[error("actor '{actor}' has permission '{permission}', but no performed usecase or invoked API path currently requires it\n  hint: remove `has_permission({actor}, {permission})`, add the missing `requires_permission` on the operation that needs it, or keep it as an intentional out-of-model grant")]
    ActorPermissionExcess { actor: String, permission: String },

    // ── イベント整合性診断 ────────────────────────────────────────────────────
    #[error("event '{event}' is never raised by any use case\n  hint: add `raises(usecase::Foo, event::{event})` in the relevant BUC file")]
    EventNeverRaised { event: String },

    #[error("event '{event}' is raised but triggers no use case or BUC\n  hint: add `triggers(event::{event}, SomeUseCase)` or `triggers(event::{event}, SomeBuc)` to connect this event; use `outbox(event::{event})` if it is intentionally published outside this model, or delete it if nothing consumes it (a use case drives `transitions` directly)")]
    EventNeverConsumed { event: String },

    #[error("event '{event}' triggers use case '{usecase}' which belongs to no BUC\n  hint: add `contains(someBuc, usecase::{usecase})` to include the triggered use case in a BUC")]
    TriggeredUseCaseUnreachable { event: String, usecase: String },

    #[error("timer '{timer}' triggers use case '{usecase}' which belongs to no BUC\n  hint: add `contains(someBuc, usecase::{usecase})` to include the scheduled use case in a BUC")]
    TimerTriggeredUseCaseUnreachable { timer: String, usecase: String },

    // ── API 整合性診断 ────────────────────────────────────────────────────────
    #[error("api '{api}' is declared but never invoked by any use case\n  hint: add `invokes(usecase::Foo, api::{api})` in the relevant BUC file")]
    ApiNeverInvoked { api: String },

    #[error("api '{api}' is invoked but operates no entity\n  hint: add `creates/updates/reads(api::{api}, SomeEntity)` to give the API work to do")]
    ApiInvokedButNoEntity { api: String },

    // ── System 境界診断 ─────────────────────────────────────────────────────
    #[error("api '{api}' belongs to multiple systems ({systems})\n  hint: an API should belong to exactly one system boundary")]
    ApiInMultipleSystems { api: String, systems: String },

    #[error("entity '{entity}' is operated by APIs in multiple systems ({systems})\n  hint: split ownership or coordinate access through use cases instead of sharing the entity")]
    EntityInMultipleSystems { entity: String, systems: String },

    #[error("entity '{entity}' is explicitly owned by multiple systems ({systems})\n  hint: keep `owns(System, Entity)` singular, or split the entity into separate owned concepts")]
    EntityOwnedByMultipleSystems { entity: String, systems: String },

    #[error("system '{system}' owns entity '{entity}', but no API in that system operates it\n  hint: add an API operation such as `reads(SomeApi, {entity})`, or keep the ownership as a deliberate future boundary")]
    OwnedEntityWithoutApiOperation { system: String, entity: String },

    #[error("api '{api}' in system '{api_system}' operates entity '{entity}' owned by system(s) {owner_systems}\n  hint: move the API under the owning system, add coordination, or revise `owns` if the declared ownership is wrong")]
    ApiOperatesEntityOutsideOwner {
        api: String,
        api_system: String,
        entity: String,
        owner_systems: String,
    },

    #[error("relation crosses system boundary without use case coordination: entity '{from}' in system '{from_system}' relates to entity '{to}' in system '{to_system}'\n  hint: add `coordinates(SomeUseCase, {from}, {to})` and invoke APIs on both system sides")]
    CrossSystemEntityRelation {
        from: String,
        from_system: String,
        to: String,
        to_system: String,
    },

    #[error("usecase '{usecase}' coordinates '{from}' and '{to}', but this pair is not a relation crossing two derived system boundaries\n  hint: use `coordinates` only for entity relations that cross system boundaries")]
    CoordinationNotCrossSystem {
        usecase: String,
        from: String,
        to: String,
    },

    #[error("usecase '{usecase}' coordinates entity '{entity}' in system '{system}', but invokes no API in that system operating the entity\n  hint: add `invokes({usecase}, SomeApi)` where `contains({system}, SomeApi)` and the API reads or writes '{entity}'")]
    CoordinationMissingApi {
        usecase: String,
        entity: String,
        system: String,
    },

    #[error("invalid temporal property '{id}': {message}\n  hint: use always/eventually/leads_to with ==/!= or Int comparisons (<,>,<=,>=) over entity columns, combining with and / or / not (`/\\` `\\/` `~` remain aliases)")]
    InvalidTemporalProperty { id: String, message: String },

    #[error("unknown predicate '{name}'\n  hint: check spelling; known predicates include performs, reads, sets, transitions, forbidden, …")]
    UnknownPredicate { name: String },

    #[error("predicate '{name}' expects {expected} argument(s), got {got}\n  hint: check the predicate signature in the language reference")]
    WrongArity {
        name: String,
        expected: String,
        got: usize,
    },

    #[error("duplicate column '{col}' on {kind} '{id}'\n  hint: each column/field name must be unique within the declaration")]
    DuplicateColumn {
        kind: String,
        id: String,
        col: String,
    },

    #[error("duplicate temporal property id '{id}'\n  hint: each `property` declaration must have a unique id")]
    DuplicateProperty { id: String },

    #[error("unknown chain method '.{name}' on predicate '{pred}'\n  hint: {hint}")]
    UnknownChainMethod {
        pred: String,
        name: String,
        hint: String,
    },

    #[error("{message}\n  hint: {hint}")]
    LintFinding {
        rule: &'static str,
        message: String,
        hint: String,
    },
}

#[derive(Debug)]
pub struct Diagnostic {
    pub error: RdraError,
    pub is_warning: bool,
    pub location: Option<crate::location::LocatedSpan>,
}

/// Hint for `TypeMismatch`: an event passed where `transitions` or `sets` now takes a use case
/// names the migration; every other mismatch keeps the generic hint.
fn type_mismatch_hint(pred: &str, id: &str, actual: &str) -> String {
    if actual == "event" && matches!(pred, "transitions" | "sets") {
        let event = id.strip_prefix("event::").unwrap_or(id);
        format!(
            "`{pred}` takes the use case that drives the change, not an event; name the use case that raises `{event}`, or run `scripts/migrate-phase3.py` to rewrite the model"
        )
    } else {
        "check that you are passing the right element type".to_string()
    }
}

impl Diagnostic {
    pub fn error(e: RdraError) -> Self {
        Self {
            error: e,
            is_warning: false,
            location: None,
        }
    }

    pub fn warning(e: RdraError) -> Self {
        Self {
            error: e,
            is_warning: true,
            location: None,
        }
    }

    pub fn error_at(e: RdraError, location: crate::location::LocatedSpan) -> Self {
        Self {
            error: e,
            is_warning: false,
            location: Some(location),
        }
    }

    pub fn warning_at(e: RdraError, location: crate::location::LocatedSpan) -> Self {
        Self {
            error: e,
            is_warning: true,
            location: Some(location),
        }
    }
}
