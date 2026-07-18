// chumsky の select! マクロが生成する Simple<Token> クロージャは型サイズが大きく
// result_large_err が出るが、サードパーティ型のため制御不能。
#![allow(clippy::result_large_err)]

use chumsky::prelude::*;
use chumsky::Stream;
use logos::Logos;

use crate::ast::*;
use crate::token::Token;

// ── Lexer bridge ─────────────────────────────────────────────────────────────

/// Run the logos lexer and return a `Vec<(Token, Span)>`.
/// Tokens that fail to lex are silently dropped.
pub fn lex(src: &str) -> Vec<Spanned<Token>> {
    Token::lexer(src)
        .spanned()
        .filter_map(|(tok, span)| tok.ok().map(|t| (t, span)))
        .collect()
}

// ── Parser helpers ────────────────────────────────────────────────────────────

/// Match a single `Token::Ident` and return the inner string.
fn ident() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::Ident(s) => s }
}

/// Match a single `Token::StringLit` and return the inner string (quotes stripped).
fn string_lit() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::StringLit(s) => s }
}

/// Match a token that can be used as a module/import path segment.
///
/// Path segments are namespace labels, so keyword-like names such as
/// `buc.purchase` must remain valid even though `buc` is a declaration keyword
/// elsewhere in the grammar.
fn path_ident() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! {
        Token::Ident(s) => s,
        Token::Actor => "actor".to_string(),
        Token::ExtSystem => "extsystem".to_string(),
        Token::System => "system".to_string(),
        Token::Requirement => "requirement".to_string(),
        Token::Adr => "adr".to_string(),
        Token::Nfr => "nfr".to_string(),
        Token::Quality => "quality".to_string(),
        Token::Constraint => "constraint".to_string(),
        Token::Concept => "concept".to_string(),
        Token::DomainObject => "domain_object".to_string(),
        Token::Aggregate => "aggregate".to_string(),
        Token::ValueObject => "valueobject".to_string(),
        Token::Business => "business".to_string(),
        Token::Buc => "buc".to_string(),
        Token::Flow => "flow".to_string(),
        Token::Step => "step".to_string(),
        Token::UsageScene => "usagescene".to_string(),
        Token::UseCase => "usecase".to_string(),
        Token::Screen => "screen".to_string(),
        Token::Field => "field".to_string(),
        Token::Event => "event".to_string(),
        Token::Entity => "entity".to_string(),
        Token::State => "state".to_string(),
        Token::Condition => "condition".to_string(),
        Token::Variation => "variation".to_string(),
        Token::Api => "api".to_string(),
        Token::Dto => "dto".to_string(),
        Token::Location => "location".to_string(),
        Token::Timing => "timing".to_string(),
        Token::Medium => "medium".to_string(),
        Token::Permission => "permission".to_string(),
        Token::Property => "property".to_string(),
    }
}

// ── Dotted name ───────────────────────────────────────────────────────────────

/// `foo.bar.baz`
fn dotted_name() -> impl Parser<Token, DottedName, Error = Simple<Token>> + Clone {
    path_ident()
        .then(just(Token::Dot).ignore_then(path_ident()).repeated())
        .map(|(head, tail)| {
            let mut parts = vec![head];
            parts.extend(tail);
            DottedName(parts)
        })
}

// ── Import ────────────────────────────────────────────────────────────────────

/// `Customer` or `Customer as C`
fn select_item() -> impl Parser<Token, SelectItem, Error = Simple<Token>> + Clone {
    ident()
        .map_with_span(|name, span| (name, span))
        .then(just(Token::As).ignore_then(ident()).or_not())
        .map_with_span(|((name, _name_span), alias), span| SelectItem { name, alias, span })
}

/// Parse one `import` declaration.
/// Grammar:
///   import <dotted_name>
///   import <dotted_name> as <ident>
///   import <dotted_name>.{<select_item> (, <select_item>)*}
///
/// The path is always the ident-dot sequence *before* any `as` or `.{`.
/// Because dotted_name greedily consumes all ident.ident segments, a selective
/// import like `shared.actors.{Customer}` needs the path built without the final
/// dot, and the brace suffix is matched after. We handle this by matching the
/// path directly as ident segments and then dispatching on the suffix.
fn import_decl() -> impl Parser<Token, ImportDecl, Error = Simple<Token>> + Clone {
    // One ident segment (not inside braces).
    let path_segment = path_ident();

    // dotted sequence: head (. segment)*
    let path = path_segment
        .clone()
        .then(
            just(Token::Dot)
                .ignore_then(path_segment.clone())
                .repeated(),
        )
        .map(|(head, tail): (String, Vec<String>)| {
            let mut parts = vec![head];
            parts.extend(tail);
            DottedName(parts)
        });

    // `.{item, ...}` suffix
    let select_suffix = just(Token::Dot)
        .ignore_then(just(Token::LBrace))
        .ignore_then(
            select_item()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RBrace));

    // `as <ident>` suffix
    let alias_suffix = just(Token::As).ignore_then(ident());

    // suffix: SelectItems | Alias | nothing
    let suffix = select_suffix
        .map(ImportKind::Select)
        .or(alias_suffix.map(ImportKind::Alias))
        .or_not()
        .map(|opt| opt.unwrap_or(ImportKind::All));

    just(Token::Import)
        .ignore_then(path.then(suffix))
        .map_with_span(|(path, kind), span| ImportDecl { path, kind, span })
}

// ── Column annotations ────────────────────────────────────────────────────────

/// An ident, string-lit, or int-lit used as an annotation argument value.
fn ann_value() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    let int_lit = select! { Token::IntLit(s) => s };
    ident().or(string_lit()).or(int_lit)
}

fn annotation() -> impl Parser<Token, Annotation, Error = Simple<Token>> + Clone {
    let annotation_args = || {
        just(Token::LParen)
            .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
            .then_ignore(just(Token::RParen))
            .or_not()
    };

    // @pk  or  @pk(a, b)
    let at_pk = just(Token::AtPk)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Pk,
            Some(v) if v.is_empty() => Annotation::Pk,
            Some(v) => Annotation::PkComposite(v),
        });

    // @unique  or  @unique(a, b)
    let at_unique = just(Token::AtUnique)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Unique,
            Some(v) if v.is_empty() => Annotation::Unique,
            Some(v) => Annotation::UniqueComposite(v),
        });

    // @index  or  @index(a, b)
    let at_index = just(Token::AtIndex)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Index,
            Some(v) if v.is_empty() => Annotation::Index,
            Some(v) => Annotation::IndexComposite(v),
        });

    // @check("...")
    let at_check = just(Token::AtCheck)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Check);

    // @null
    let at_null = just(Token::AtNull).to(Annotation::Null);

    // @default(value)
    let at_default = just(Token::AtDefault)
        .ignore_then(just(Token::LParen))
        .ignore_then(ann_value())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Default);

    // @label("...")
    let at_label = just(Token::AtLabel)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Label);

    let at_soft_delete = just(Token::AtSoftDelete).to(Annotation::SoftDelete);
    let at_history = just(Token::AtHistory).to(Annotation::History);
    let at_tenant = just(Token::AtTenant).to(Annotation::Tenant);
    let at_derived = just(Token::AtDerived)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Derived);

    choice((
        at_pk,
        at_unique,
        at_index,
        at_check,
        at_null,
        at_default,
        at_label,
        at_soft_delete,
        at_history,
        at_tenant,
        at_derived,
    ))
}

// ── Column type ───────────────────────────────────────────────────────────────

fn col_type() -> impl Parser<Token, ColType, Error = Simple<Token>> + Clone {
    let simple = select! {
        Token::TInt      => ColType::Int,
        Token::TString   => ColType::String,
        Token::TMoney    => ColType::Money,
        Token::TDateTime => ColType::DateTime,
        Token::TDate     => ColType::Date,
        Token::TBool     => ColType::Bool,
        Token::TDecimal  => ColType::Decimal,
    };

    // Enum(active, discontinued)
    let enum_ty = just(Token::TEnum)
        .ignore_then(just(Token::LParen))
        .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
        .then_ignore(just(Token::RParen))
        .map(ColType::Enum);

    enum_ty.or(simple)
}

// ── Column definition ─────────────────────────────────────────────────────────

/// `name: Type @ann1 @ann2 ...`
fn column() -> impl Parser<Token, Column, Error = Simple<Token>> + Clone {
    ident()
        .then_ignore(just(Token::Colon))
        .then(col_type())
        .then(annotation().repeated())
        .map_with_span(|((name, col_type), annotations), span| Column {
            name,
            col_type,
            annotations,
            span,
        })
}

// ── Instance declaration ──────────────────────────────────────────────────────

fn kind_token() -> impl Parser<Token, Kind, Error = Simple<Token>> + Clone {
    select! {
        Token::Actor       => Kind::Actor,
        Token::ExtSystem   => Kind::ExtSystem,
        Token::System      => Kind::System,
        Token::Requirement => Kind::Requirement,
        Token::Adr         => Kind::Adr,
        Token::Nfr         => Kind::Nfr,
        Token::Quality     => Kind::Quality,
        Token::Constraint  => Kind::Constraint,
        Token::Concept     => Kind::Concept,
        Token::DomainObject => Kind::DomainObject,
        Token::Aggregate   => Kind::Aggregate,
        Token::ValueObject => Kind::ValueObject,
        Token::Business    => Kind::Business,
        Token::Buc         => Kind::Buc,
        Token::Flow        => Kind::Flow,
        Token::Step        => Kind::Step,
        Token::UsageScene  => Kind::UsageScene,
        Token::UseCase     => Kind::UseCase,
        Token::Screen      => Kind::Screen,
        Token::Field       => Kind::Field,
        Token::Event       => Kind::Event,
        Token::Entity      => Kind::Entity,
        Token::State       => Kind::State,
        Token::Condition   => Kind::Condition,
        Token::Variation   => Kind::Variation,
        Token::Api         => Kind::Api,
        Token::Dto         => Kind::Dto,
        Token::Location    => Kind::Location,
        Token::Timing      => Kind::Timing,
        Token::Medium      => Kind::Medium,
        Token::Permission  => Kind::Permission,
    }
}

fn instance_decl() -> impl Parser<Token, InstanceDecl, Error = Simple<Token>> + Clone {
    let body = just(Token::LBrace)
        .ignore_then(column().repeated())
        .then_ignore(just(Token::RBrace));

    let description = just(Token::Ident("description".to_string()))
        .ignore_then(string_lit())
        .or_not();

    let requirement_metadata = requirement_metadata().or_not();
    let adr_metadata = adr_metadata().or_not();
    let api_metadata = api_metadata().or_not();
    let nfr_metadata = nfr_metadata().or_not();
    let field_metadata = field_metadata().or_not();
    let usecase_metadata = usecase_metadata().or_not();

    kind_token()
        .then(ident())
        .then(string_lit())
        .then(description)
        .then(requirement_metadata)
        .then(adr_metadata)
        .then(api_metadata)
        .then(nfr_metadata)
        .then(field_metadata)
        .then(usecase_metadata)
        .then(body.or_not())
        .map_with_span(
            |(
                (
                    ((((((((kind, id), label), description), requirement), adr), api), nfr), field),
                    usecase,
                ),
                columns,
            ),
             span| {
                InstanceDecl {
                    kind,
                    id,
                    label,
                    description,
                    requirement: requirement.unwrap_or_default(),
                    adr: adr.unwrap_or_default(),
                    api: api.unwrap_or_default(),
                    nfr: nfr.unwrap_or_default(),
                    field: field.unwrap_or_default(),
                    usecase: usecase.unwrap_or_default(),
                    columns: columns.unwrap_or_default(),
                    span,
                }
            },
        )
}

#[derive(Debug, Clone, PartialEq)]
enum AdrMetadataEntry {
    Status(String),
    Context(String),
    Decision(String),
    Consequence(String),
    AcceptedOption(String),
    RejectedOption(String),
    Reason(String),
}

fn adr_metadata_entry() -> impl Parser<Token, AdrMetadataEntry, Error = Simple<Token>> + Clone {
    let status = just(Token::Ident("adr_status".to_string()))
        .ignore_then(metadata_value())
        .map(AdrMetadataEntry::Status);
    let context = just(Token::Ident("context".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::Context);
    let decision = just(Token::Ident("decision".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::Decision);
    let consequence = just(Token::Ident("consequence".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::Consequence);
    let accepted = just(Token::Ident("accepted".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::AcceptedOption);
    let accepted_option = just(Token::Ident("accepted_option".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::AcceptedOption);
    let rejected = just(Token::Ident("rejected".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::RejectedOption);
    let rejected_option = just(Token::Ident("rejected_option".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::RejectedOption);
    let reason = just(Token::Ident("reason".to_string()))
        .ignore_then(string_lit())
        .map(AdrMetadataEntry::Reason);

    status
        .or(context)
        .or(decision)
        .or(consequence)
        .or(accepted_option)
        .or(accepted)
        .or(rejected_option)
        .or(rejected)
        .or(reason)
}

fn adr_metadata() -> impl Parser<Token, AdrMetadata, Error = Simple<Token>> + Clone {
    adr_metadata_entry().repeated().at_least(1).map(|entries| {
        let mut metadata = AdrMetadata::default();
        for entry in entries {
            match entry {
                AdrMetadataEntry::Status(value) => metadata.status = Some(value),
                AdrMetadataEntry::Context(value) => metadata.context.push(value),
                AdrMetadataEntry::Decision(value) => metadata.decision = Some(value),
                AdrMetadataEntry::Consequence(value) => metadata.consequences.push(value),
                AdrMetadataEntry::AcceptedOption(value) => metadata.accepted_options.push(value),
                AdrMetadataEntry::RejectedOption(value) => metadata.rejected_options.push(value),
                AdrMetadataEntry::Reason(value) => metadata.reasons.push(value),
            }
        }
        metadata
    })
}

#[derive(Debug, Clone, PartialEq)]
enum RequirementMetadataEntry {
    Priority(String),
    Source(String),
    Stakeholder(String),
    Owner(String),
    AcceptanceCriterion(String),
    Status(String),
    Risk(String),
    Rationale(String),
}

fn requirement_metadata_entry(
) -> impl Parser<Token, RequirementMetadataEntry, Error = Simple<Token>> + Clone {
    let priority = just(Token::Ident("priority".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Priority);
    let source = just(Token::Ident("source".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Source);
    let stakeholder = just(Token::Ident("stakeholder".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Stakeholder);
    let owner = just(Token::Ident("owner".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Owner);
    let acceptance = just(Token::Ident("acceptance".to_string()))
        .ignore_then(just(Token::Ident("criteria".to_string())).or_not())
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::AcceptanceCriterion);
    let acceptance_criteria = just(Token::Ident("acceptance_criteria".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::AcceptanceCriterion);
    let status = just(Token::Ident("status".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Status);
    let risk = just(Token::Ident("risk".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Risk);
    let rationale = just(Token::Ident("rationale".to_string()))
        .ignore_then(string_lit())
        .map(RequirementMetadataEntry::Rationale);

    priority
        .or(source)
        .or(stakeholder)
        .or(owner)
        .or(acceptance_criteria)
        .or(acceptance)
        .or(status)
        .or(risk)
        .or(rationale)
}

fn requirement_metadata() -> impl Parser<Token, RequirementMetadata, Error = Simple<Token>> + Clone
{
    requirement_metadata_entry()
        .repeated()
        .at_least(1)
        .map(|entries| {
            let mut metadata = RequirementMetadata::default();
            for entry in entries {
                match entry {
                    RequirementMetadataEntry::Priority(value) => metadata.priority = Some(value),
                    RequirementMetadataEntry::Source(value) => metadata.sources.push(value),
                    RequirementMetadataEntry::Stakeholder(value) => {
                        metadata.stakeholders.push(value)
                    }
                    RequirementMetadataEntry::Owner(value) => metadata.owner = Some(value),
                    RequirementMetadataEntry::AcceptanceCriterion(value) => {
                        metadata.acceptance_criteria.push(value);
                    }
                    RequirementMetadataEntry::Status(value) => metadata.status = Some(value),
                    RequirementMetadataEntry::Risk(value) => metadata.risk = Some(value),
                    RequirementMetadataEntry::Rationale(value) => metadata.rationale = Some(value),
                }
            }
            metadata
        })
}

#[derive(Debug, Clone, PartialEq)]
enum ApiMetadataEntry {
    Method(String),
    Path(String),
    Idempotency(String),
    Mode(String),
    AuthScheme(String),
}

fn metadata_value() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    path_ident().or(string_lit())
}

fn api_metadata_entry() -> impl Parser<Token, ApiMetadataEntry, Error = Simple<Token>> + Clone {
    let method = just(Token::Ident("method".to_string()))
        .ignore_then(metadata_value())
        .map(ApiMetadataEntry::Method);
    let path = just(Token::Ident("path".to_string()))
        .ignore_then(string_lit())
        .map(ApiMetadataEntry::Path);
    let idempotency = just(Token::Ident("idempotency".to_string()))
        .ignore_then(metadata_value())
        .map(ApiMetadataEntry::Idempotency);
    let mode = just(Token::Ident("mode".to_string()))
        .ignore_then(metadata_value())
        .map(ApiMetadataEntry::Mode);
    let auth = just(Token::Ident("auth".to_string()))
        .ignore_then(metadata_value())
        .map(ApiMetadataEntry::AuthScheme);
    let auth_scheme = just(Token::Ident("auth_scheme".to_string()))
        .ignore_then(metadata_value())
        .map(ApiMetadataEntry::AuthScheme);

    method
        .or(path)
        .or(idempotency)
        .or(mode)
        .or(auth_scheme)
        .or(auth)
}

fn api_metadata() -> impl Parser<Token, ApiMetadata, Error = Simple<Token>> + Clone {
    api_metadata_entry().repeated().at_least(1).map(|entries| {
        let mut metadata = ApiMetadata::default();
        for entry in entries {
            match entry {
                ApiMetadataEntry::Method(value) => metadata.method = Some(value),
                ApiMetadataEntry::Path(value) => metadata.path = Some(value),
                ApiMetadataEntry::Idempotency(value) => metadata.idempotency = Some(value),
                ApiMetadataEntry::Mode(value) => metadata.mode = Some(value),
                ApiMetadataEntry::AuthScheme(value) => metadata.auth_scheme = Some(value),
            }
        }
        metadata
    })
}

#[derive(Debug, Clone, PartialEq)]
enum NfrMetadataEntry {
    Metric(String),
    Target(String),
    Window(String),
    Slo(String),
    Availability(String),
    Resilience(String),
    Audit(String),
    Logging(String),
    Retention(String),
    Privacy(String),
}

fn nfr_metadata_entry() -> impl Parser<Token, NfrMetadataEntry, Error = Simple<Token>> + Clone {
    let metric = just(Token::Ident("metric".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Metric);
    let target = just(Token::Ident("target".to_string()))
        .ignore_then(string_lit())
        .map(NfrMetadataEntry::Target);
    let window = just(Token::Ident("window".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Window);
    let slo = just(Token::Ident("slo".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Slo);
    let availability = just(Token::Ident("availability".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Availability);
    let resilience = just(Token::Ident("resilience".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Resilience);
    let audit = just(Token::Ident("audit".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Audit);
    let logging = just(Token::Ident("logging".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Logging);
    let retention = just(Token::Ident("retention".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Retention);
    let privacy = just(Token::Ident("privacy".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Privacy);
    let privacy_classification = just(Token::Ident("privacy_classification".to_string()))
        .ignore_then(metadata_value())
        .map(NfrMetadataEntry::Privacy);

    metric
        .or(target)
        .or(window)
        .or(slo)
        .or(availability)
        .or(resilience)
        .or(audit)
        .or(logging)
        .or(retention)
        .or(privacy_classification)
        .or(privacy)
}

fn nfr_metadata() -> impl Parser<Token, NfrMetadata, Error = Simple<Token>> + Clone {
    nfr_metadata_entry().repeated().at_least(1).map(|entries| {
        let mut metadata = NfrMetadata::default();
        for entry in entries {
            match entry {
                NfrMetadataEntry::Metric(value) => metadata.metric = Some(value),
                NfrMetadataEntry::Target(value) => metadata.target = Some(value),
                NfrMetadataEntry::Window(value) => metadata.window = Some(value),
                NfrMetadataEntry::Slo(value) => metadata.slo = Some(value),
                NfrMetadataEntry::Availability(value) => metadata.availability = Some(value),
                NfrMetadataEntry::Resilience(value) => metadata.resilience = Some(value),
                NfrMetadataEntry::Audit(value) => metadata.audit = Some(value),
                NfrMetadataEntry::Logging(value) => metadata.logging = Some(value),
                NfrMetadataEntry::Retention(value) => metadata.retention = Some(value),
                NfrMetadataEntry::Privacy(value) => metadata.privacy = Some(value),
            }
        }
        metadata
    })
}

#[derive(Debug, Clone, PartialEq)]
enum FieldMetadataEntry {
    Access(String),
    Required(bool),
    Source(String),
}

fn bool_metadata_value() -> impl Parser<Token, bool, Error = Simple<Token>> + Clone {
    just(Token::Ident("true".to_string()))
        .to(true)
        .or(just(Token::Ident("false".to_string())).to(false))
}

fn field_metadata_entry() -> impl Parser<Token, FieldMetadataEntry, Error = Simple<Token>> + Clone {
    let access = just(Token::Ident("access".to_string()))
        .ignore_then(metadata_value())
        .map(FieldMetadataEntry::Access);
    let required = just(Token::Ident("required".to_string()))
        .ignore_then(bool_metadata_value())
        .map(FieldMetadataEntry::Required);
    let source = just(Token::Ident("source".to_string()))
        .ignore_then(metadata_value())
        .map(FieldMetadataEntry::Source);
    let input = just(Token::Ident("input".to_string()))
        .ignore_then(metadata_value())
        .map(FieldMetadataEntry::Source);
    let derived = just(Token::Ident("derived".to_string()))
        .ignore_then(metadata_value())
        .map(FieldMetadataEntry::Source);

    access.or(required).or(source).or(input).or(derived)
}

fn field_metadata() -> impl Parser<Token, FieldMetadata, Error = Simple<Token>> + Clone {
    field_metadata_entry()
        .repeated()
        .at_least(1)
        .map(|entries| {
            let mut metadata = FieldMetadata::default();
            for entry in entries {
                match entry {
                    FieldMetadataEntry::Access(value) => metadata.access = Some(value),
                    FieldMetadataEntry::Required(value) => metadata.required = Some(value),
                    FieldMetadataEntry::Source(value) => metadata.source = Some(value),
                }
            }
            metadata
        })
}

#[derive(Debug, Clone, PartialEq)]
enum UseCaseMetadataEntry {
    Precondition(String),
    Postcondition(String),
    Guard(String),
    Alternative(String),
    Error(String),
}

fn usecase_metadata_entry(
) -> impl Parser<Token, UseCaseMetadataEntry, Error = Simple<Token>> + Clone {
    let precondition = just(Token::Ident("precondition".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Precondition);
    let postcondition = just(Token::Ident("postcondition".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Postcondition);
    let guard = just(Token::Ident("guard".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Guard);
    let alternative = just(Token::Ident("alternative".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Alternative);
    let alternative_flow = just(Token::Ident("alternative_flow".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Alternative);
    let error = just(Token::Ident("error".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Error);
    let error_condition = just(Token::Ident("error_condition".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Error);
    let business_error = just(Token::Ident("business_error".to_string()))
        .ignore_then(string_lit())
        .map(UseCaseMetadataEntry::Error);

    precondition
        .or(postcondition)
        .or(guard)
        .or(alternative_flow)
        .or(alternative)
        .or(error_condition)
        .or(business_error)
        .or(error)
}

fn usecase_metadata() -> impl Parser<Token, UseCaseMetadata, Error = Simple<Token>> + Clone {
    usecase_metadata_entry()
        .repeated()
        .at_least(1)
        .map(|entries| {
            let mut metadata = UseCaseMetadata::default();
            for entry in entries {
                match entry {
                    UseCaseMetadataEntry::Precondition(value) => {
                        metadata.preconditions.push(value);
                    }
                    UseCaseMetadataEntry::Postcondition(value) => {
                        metadata.postconditions.push(value);
                    }
                    UseCaseMetadataEntry::Guard(value) => metadata.guards.push(value),
                    UseCaseMetadataEntry::Alternative(value) => metadata.alternatives.push(value),
                    UseCaseMetadataEntry::Error(value) => metadata.errors.push(value),
                }
            }
            metadata
        })
}

// ── Qualified reference ───────────────────────────────────────────────────────

/// Parse a reference to a declared element.
///
/// Two forms are accepted:
///   `usecase::Browse`       — kind-qualified (resolves unambiguously when
///                             the same identifier is used for multiple kinds)
///   `Foo` or `a.Foo`        — plain or namespace-qualified (existing syntax)
fn qref() -> impl Parser<Token, QRef, Error = Simple<Token>> + Clone {
    // Typed form: `<kind_keyword> :: <ident>`
    let typed = kind_token()
        .then_ignore(just(Token::ColonColon))
        .then(ident())
        .map_with_span(|(kind, name), span| QRef {
            kind_qualifier: Some(kind),
            parts: vec![name],
            span,
        });

    // Plain form: `ident ("." ident)*`
    let plain = ident()
        .then(just(Token::Dot).ignore_then(ident()).repeated())
        .map_with_span(|(head, tail), span| {
            let mut parts = vec![head];
            parts.extend(tail);
            QRef {
                kind_qualifier: None,
                parts,
                span,
            }
        });

    typed.or(plain)
}

// ── Predicate call ────────────────────────────────────────────────────────────

/// リテラルまたは修飾参照。
fn predicate_atom() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let lit = string_lit().map(PredicateArg::Lit);
    let r = qref().map(PredicateArg::Ref);
    lit.or(r)
}

/// `N:1` / `1:N` / `1:1` / `N:M` カーディナリティ（非クォート）。
fn card_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let side = select! {
        Token::Ident(s) if s == "N" || s == "M" => s,
        Token::IntLit(s) if s == "1" => s,
    };
    side.then_ignore(just(Token::Colon))
        .then(side)
        .map(|(left, right)| PredicateArg::Card(format!("{left}:{right}")))
}

/// `pending -> paid` 遷移端点。
fn transition_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    ident()
        .then_ignore(just(Token::Arrow))
        .then(ident())
        .map(|(from, to)| PredicateArg::Transition { from, to })
}

/// 比較式の被演算子（Operand）:
/// 裸ident（カラム参照）、`Entity.column`、整数リテラル、または `now`。
fn operand() -> impl Parser<Token, Operand, Error = Simple<Token>> + Clone {
    let now = just(Token::Now).map(|_| Operand::Now);
    let int_lit = select! { Token::IntLit(s) => Operand::IntLit(s) };
    let qualified_col = ident()
        .map_with_span(|entity, span| {
            (
                entity.clone(),
                QRef {
                    kind_qualifier: None,
                    parts: vec![entity],
                    span,
                },
            )
        })
        .then_ignore(just(Token::Dot))
        .then(ident())
        .map_with_span(|((_entity_name, entity), column), span| {
            Operand::QualifiedColumn(QualifiedColumnRef {
                entity,
                column,
                span,
            })
        });
    let col = ident().map(Operand::Column);
    // `now` must come before generic ident because logos lexes it as Token::Now
    now.or(int_lit).or(qualified_col).or(col)
}

/// 比較演算子トークン → `CmpOp`
fn cmp_op() -> impl Parser<Token, CmpOp, Error = Simple<Token>> + Clone {
    select! {
        Token::Le  => CmpOp::Le,
        Token::Ge  => CmpOp::Ge,
        Token::EqEq => CmpOp::Eq,
        Token::Ne  => CmpOp::Ne,
        Token::Lt  => CmpOp::Lt,
        Token::Gt  => CmpOp::Gt,
    }
}

/// 比較式: `operand cmp_op operand`（例: `stock < selling`, `expired_at < now`）
fn comparison() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    operand()
        .then(cmp_op())
        .then(operand())
        .map_with_span(|((lhs, op), rhs), span| Expr::Cmp(Comparison { lhs, op, rhs, span }))
}

/// Logical expression for temporal properties: `and`/`or`/`not` (TLA `/\` `\/` `~` are aliases).
fn logical_expr() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    recursive(|expr| {
        let atom = comparison().or(just(Token::LParen)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::RParen)));

        let not_op = just(Token::Not).or(just(Token::TlaNot));
        let not_expr = not_op
            .ignore_then(atom.clone())
            .map(|inner| Expr::Not(Box::new(inner)))
            .or(atom);

        let and_op = just(Token::And).or(just(Token::TlaAnd));
        let and_expr = not_expr
            .clone()
            .then(and_op.ignore_then(not_expr.clone()).repeated())
            .map(|(first, rest)| {
                rest.into_iter()
                    .fold(first, |acc, next| Expr::And(Box::new(acc), Box::new(next)))
            });

        let or_op = just(Token::Or).or(just(Token::TlaOr));
        and_expr
            .clone()
            .then(or_op.ignore_then(and_expr.clone()).repeated())
            .map(|(first, rest)| {
                rest.into_iter()
                    .fold(first, |acc, next| Expr::Or(Box::new(acc), Box::new(next)))
            })
    })
}

fn property_decl() -> impl Parser<Token, PropertyDecl, Error = Simple<Token>> + Clone {
    let always = just(Token::Ident("always".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(AstTemporalFormula::Always);

    let eventually = just(Token::Ident("eventually".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(AstTemporalFormula::Eventually);

    let leads_to = just(Token::Ident("leads_to".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::Comma))
        .then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(|(antecedent, consequent)| AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        });

    let formula = choice((leads_to, always, eventually));

    just(Token::Property)
        .ignore_then(ident())
        .then(string_lit())
        .then(formula)
        .map_with_span(|((id, label), formula), span| PropertyDecl {
            id,
            label,
            formula,
            span,
        })
}

/// 引数: 比較式、遷移端点、カーディナリティ、文字列リテラル、または修飾参照。
///
/// 比較式を **最優先** でパースし、続けて遷移 / カード / atom にフォールバックする。
/// - `forbidden(E, status == cancelled)` → `PredicateArg::Expr`
/// - `transitions(E.col, Ev, a -> b)` の第3引数 → `PredicateArg::Transition`
/// - `relate(A, B, N:1)` の第3引数 → `PredicateArg::Card`
/// - `performs(A, B)` → `PredicateArg::Ref`
fn predicate_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let expr = comparison().map(PredicateArg::Expr);
    expr.or(transition_arg())
        .or(card_arg())
        .or(predicate_atom())
}

/// `.method(args...)` のチェーン呼び出し1件。
fn chain_call() -> impl Parser<Token, ChainCall, Error = Simple<Token>> + Clone {
    just(Token::Dot)
        .ignore_then(ident())
        .then_ignore(just(Token::LParen))
        .then(
            predicate_arg()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RParen))
        .map_with_span(|(name, args), span| ChainCall { name, args, span })
}

fn predicate_call() -> impl Parser<Token, PredicateCall, Error = Simple<Token>> + Clone {
    ident()
        .then_ignore(just(Token::LParen))
        .then(
            predicate_arg()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RParen))
        .then(chain_call().repeated())
        .map_with_span(|((name, args), chain), span| PredicateCall {
            name,
            args,
            chain,
            span,
        })
}

// ── Module declaration ────────────────────────────────────────────────────────

fn module_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    just(Token::Module)
        .ignore_then(dotted_name())
        .map_with_span(Item::Module)
}

// ── Top-level item ────────────────────────────────────────────────────────────

fn item() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let import = import_decl().map(Item::Import);
    let instance = instance_decl().map(Item::Instance);
    let property = property_decl().map(Item::Property);
    let predicate = predicate_call().map(Item::Predicate);

    // property before instance so `property` keyword is not consumed as Ident.
    choice((module_decl(), import, property, instance, predicate))
}

// ── Root parser ───────────────────────────────────────────────────────────────

fn root_parser() -> impl Parser<Token, Vec<Item>, Error = Simple<Token>> {
    item()
        .recover_with(skip_then_retry_until([]))
        .repeated()
        .then_ignore(end())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Lex and parse `src`. Returns the best-effort AST and any parse errors.
pub fn parse(src: &str) -> (Ast, Vec<Simple<Token>>) {
    let tokens = lex(src);
    let len = src.len();

    let stream = Stream::from_iter(len..len + 1, tokens.into_iter());
    let (items, errors) = root_parser().parse_recovery(stream);

    let ast = Ast {
        items: items.unwrap_or_default(),
        source: src.to_string(),
    };
    (ast, errors)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Ast {
        let (ast, errors) = parse(src);
        if !errors.is_empty() {
            panic!("parse errors: {:?}", errors);
        }
        ast
    }

    #[test]
    fn test_parse_instance_decl() {
        let ast = parse_ok(r#"actor Customer "顧客" description "商品を購入する顧客""#);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_requirement_metadata() {
        let ast = parse_ok(
            r#"requirement ReqCheckout "Checkout must be reliable"
description "The checkout flow must preserve customer intent."
priority "must"
source "Customer interview"
stakeholder "Store Operations"
owner "Product Owner"
acceptance criteria "A payment timeout leaves the cart recoverable."
acceptance_criteria "A submitted order receives a stable order number."
status "proposed"
risk "high"
rationale "Checkout failures directly block revenue.""#,
        );

        let Item::Instance(inst) = &ast.items[0] else {
            panic!("expected instance declaration");
        };
        assert_eq!(inst.kind, Kind::Requirement);
        assert_eq!(inst.requirement.priority.as_deref(), Some("must"));
        assert_eq!(inst.requirement.sources, vec!["Customer interview"]);
        assert_eq!(inst.requirement.stakeholders, vec!["Store Operations"]);
        assert_eq!(inst.requirement.owner.as_deref(), Some("Product Owner"));
        assert_eq!(
            inst.requirement.acceptance_criteria,
            vec![
                "A payment timeout leaves the cart recoverable.",
                "A submitted order receives a stable order number."
            ]
        );
        assert_eq!(inst.requirement.status.as_deref(), Some("proposed"));
        assert_eq!(inst.requirement.risk.as_deref(), Some("high"));
        assert_eq!(
            inst.requirement.rationale.as_deref(),
            Some("Checkout failures directly block revenue.")
        );
    }

    #[test]
    fn test_parse_adr_metadata() {
        let ast = parse_ok(
            r#"adr AdrOutbox "Use transactional outbox"
adr_status accepted
context "External subscribers need customer changes."
decision "Publish customer changes through a transactional outbox."
consequence "Delivery becomes eventually consistent."
accepted "Transactional outbox"
rejected "Synchronous callback"
reason "Avoid coupling write latency to external subscribers.""#,
        );

        let Item::Instance(inst) = &ast.items[0] else {
            panic!("expected instance declaration");
        };
        assert_eq!(inst.kind, Kind::Adr);
        assert_eq!(inst.adr.status.as_deref(), Some("accepted"));
        assert_eq!(
            inst.adr.context,
            vec!["External subscribers need customer changes."]
        );
        assert_eq!(
            inst.adr.decision.as_deref(),
            Some("Publish customer changes through a transactional outbox.")
        );
        assert_eq!(
            inst.adr.consequences,
            vec!["Delivery becomes eventually consistent."]
        );
        assert_eq!(inst.adr.accepted_options, vec!["Transactional outbox"]);
        assert_eq!(inst.adr.rejected_options, vec!["Synchronous callback"]);
        assert_eq!(
            inst.adr.reasons,
            vec!["Avoid coupling write latency to external subscribers."]
        );
    }

    #[test]
    fn test_parse_usecase_metadata() {
        let ast = parse_ok(
            r#"usecase CapturePayment "Capture payment"
precondition "Order is authorized."
guard "Provider is available."
postcondition "Payment is captured."
alternative_flow "Customer changes payment method."
business_error "Authorization expires.""#,
        );

        let Item::Instance(uc) = &ast.items[0] else {
            panic!("expected usecase instance");
        };
        assert_eq!(uc.kind, Kind::UseCase);
        assert_eq!(uc.usecase.preconditions, vec!["Order is authorized."]);
        assert_eq!(uc.usecase.guards, vec!["Provider is available."]);
        assert_eq!(uc.usecase.postconditions, vec!["Payment is captured."]);
        assert_eq!(
            uc.usecase.alternatives,
            vec!["Customer changes payment method."]
        );
        assert_eq!(uc.usecase.errors, vec!["Authorization expires."]);
    }

    #[test]
    fn test_parse_business_flow_elements() {
        let ast = parse_ok(
            r#"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
step AuthorizePayment "Authorize payment"
contains(CheckoutFlow, ReviewCart)
precedes(ReviewCart, AuthorizePayment)
branches(ReviewCart, AuthorizePayment)
excepts(AuthorizePayment, ReviewCart)
repeats(ReviewCart, ReviewCart)
covers(AuthorizePayment, usecase::CapturePayment)
"#,
        );

        assert_eq!(ast.items.len(), 9);
        let Item::Instance(flow) = &ast.items[0] else {
            panic!("expected flow instance");
        };
        assert_eq!(flow.kind, Kind::Flow);
        let Item::Instance(step) = &ast.items[1] else {
            panic!("expected step instance");
        };
        assert_eq!(step.kind, Kind::Step);
    }

    #[test]
    fn test_parse_api_contract_elements() {
        let ast = parse_ok(
            r#"
api CreateOrder "Create order"
  method POST
  path "/orders"
  idempotency "idempotent"
  mode sync
  auth bearer
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
dto OrderResponse "Order response" {
  order_id: Int
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
"#,
        );

        let Item::Instance(api) = &ast.items[0] else {
            panic!("expected api instance");
        };
        assert_eq!(api.kind, Kind::Api);
        assert_eq!(api.api.method.as_deref(), Some("POST"));
        assert_eq!(api.api.path.as_deref(), Some("/orders"));
        assert_eq!(api.api.idempotency.as_deref(), Some("idempotent"));
        assert_eq!(api.api.mode.as_deref(), Some("sync"));
        assert_eq!(api.api.auth_scheme.as_deref(), Some("bearer"));

        let Item::Instance(dto) = &ast.items[1] else {
            panic!("expected dto instance");
        };
        assert_eq!(dto.kind, Kind::Dto);
        assert_eq!(dto.columns.len(), 2);
    }

    #[test]
    fn test_parse_non_functional_elements() {
        let ast = parse_ok(
            r#"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
quality Performance "Performance"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
applies_to(CheckoutLatency, api::CheckoutApi)
qualifies(CheckoutLatency, Performance)
constrains(AuditRetention, system::CoreSystem)
"#,
        );

        let Item::Instance(nfr) = &ast.items[0] else {
            panic!("expected nfr instance");
        };
        assert_eq!(nfr.kind, Kind::Nfr);
        assert_eq!(nfr.nfr.metric.as_deref(), Some("p95_latency_ms"));
        assert_eq!(nfr.nfr.target.as_deref(), Some("<=300"));
        assert_eq!(nfr.nfr.window.as_deref(), Some("5m"));
        assert_eq!(nfr.nfr.slo.as_deref(), Some("99.9%"));

        let Item::Instance(quality) = &ast.items[1] else {
            panic!("expected quality instance");
        };
        assert_eq!(quality.kind, Kind::Quality);

        let Item::Instance(constraint) = &ast.items[2] else {
            panic!("expected constraint instance");
        };
        assert_eq!(constraint.kind, Kind::Constraint);
        assert_eq!(constraint.nfr.retention.as_deref(), Some("7y"));
        assert_eq!(constraint.nfr.privacy.as_deref(), Some("restricted"));
    }

    #[test]
    fn test_parse_conceptual_model_elements() {
        let ast = parse_ok(
            r#"
concept PatientIdentity "Patient identity"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"
entity AppointmentTable "appointment table" { id: Int @pk }
contains(SchedulingAggregate, Appointment)
contains(SchedulingAggregate, TimeSlot)
maps_to(Appointment, AppointmentTable)
"#,
        );

        let Item::Instance(concept) = &ast.items[0] else {
            panic!("expected concept instance");
        };
        assert_eq!(concept.kind, Kind::Concept);
        let Item::Instance(domain_object) = &ast.items[1] else {
            panic!("expected domain object instance");
        };
        assert_eq!(domain_object.kind, Kind::DomainObject);
        let Item::Instance(aggregate) = &ast.items[2] else {
            panic!("expected aggregate instance");
        };
        assert_eq!(aggregate.kind, Kind::Aggregate);
        let Item::Instance(value_object) = &ast.items[3] else {
            panic!("expected value object instance");
        };
        assert_eq!(value_object.kind, Kind::ValueObject);
    }

    #[test]
    fn test_parse_screen_field_metadata() {
        let ast = parse_ok(
            r#"
field ShippingAddress "Shipping address"
  access editable
  required true
  source actor
field OrderTotal "Order total"
  access readonly
  derived system
contains(CheckoutScreen, ShippingAddress)
maps_field(ShippingAddress, Order, "shipping_address")
"#,
        );

        let Item::Instance(field) = &ast.items[0] else {
            panic!("expected field instance");
        };
        assert_eq!(field.kind, Kind::Field);
        assert_eq!(field.field.access.as_deref(), Some("editable"));
        assert_eq!(field.field.required, Some(true));
        assert_eq!(field.field.source.as_deref(), Some("actor"));
        let Item::Instance(derived_field) = &ast.items[1] else {
            panic!("expected derived field instance");
        };
        assert_eq!(derived_field.field.access.as_deref(), Some("readonly"));
        assert_eq!(derived_field.field.source.as_deref(), Some("system"));
    }

    #[test]
    fn test_parse_entity() {
        let src = r#"
entity Product "商品" {
  id:  Int    @pk
  sku: String @unique @index
  name: String @label("商品名")
  price: Decimal @check("price >= 0")
  store_id: Int @index(status, store_id) @unique(sku, store_id)
  status: Enum(active, discontinued) @default(active)
  note: String @null
  tenant_id: Int @tenant
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  display_price: Money @derived("price * tax_rate")
}
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_predicate() {
        let ast = parse_ok("performs(Customer, Browse)");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_relate() {
        let ast = parse_ok(
            r#"relate(Order, Customer, N:1).optional().on_delete(set_null).on_update(cascade)"#,
        );
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_import() {
        let src = r#"
import shared.actors
import shared.entities as e
import shared.actors.{Customer, Staff}
import shared.actors.{Customer as C}
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_module() {
        let ast = parse_ok("module shared.actors");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_module_path_with_keyword_segment() {
        let ast = parse_ok("module buc.purchase");
        assert_eq!(ast.items.len(), 1);
        let Item::Module(name, _) = &ast.items[0] else {
            panic!("expected module declaration");
        };
        assert_eq!(name.0, vec!["buc", "purchase"]);
    }

    #[test]
    fn test_parse_full_snippet() {
        let src = r#"
// コメント
module shared.actors

import shared.entities

actor   Customer "顧客"
usecase Browse   "商品を探す"

entity  Order "注文" {
  id: Int @pk
  total: Money
  ordered_at: DateTime
}

performs(Customer, Browse)
relate(Order, Customer, N:1)
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_inline_comments() {
        let src = r#"
module shop // module comment
actor Customer "顧客" /* actor comment */
usecase Browse "商品を探す" // usecase comment
entity Product "商品" { // columns
  id: Int @pk // primary key
  name: String /* display name */
  // status column
  status: Enum(active, discontinued) @default(active) // default status
}
performs(Customer, Browse) /* predicate comment */
"#;
        let ast = parse_ok(src);
        assert_eq!(ast.items.len(), 5);
    }

    #[test]
    fn test_parse_equals_forbidden() {
        let ast = parse_ok(r#"forbidden(Order, status == cancelled)"#);
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "forbidden");
        assert_eq!(pred.args.len(), 2);
        assert!(matches!(&pred.args[1], PredicateArg::Expr(Expr::Cmp(_))));
        assert!(pred.chain.is_empty());
    }

    #[test]
    fn test_parse_chained_invariant() {
        let ast =
            parse_ok(r#"invariant(Order).when(status == delivered).then(delivered_at == present)"#);
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "invariant");
        assert_eq!(pred.args.len(), 1);
        assert_eq!(pred.chain.len(), 2);
        assert_eq!(pred.chain[0].name, "when");
        assert_eq!(pred.chain[0].args.len(), 1);
        assert!(matches!(
            &pred.chain[0].args[0],
            PredicateArg::Expr(Expr::Cmp(_))
        ));
        assert_eq!(pred.chain[1].name, "then");
        assert_eq!(pred.chain[1].args.len(), 1);
    }

    #[test]
    fn test_parse_cross_forbidden_with_along_chain() {
        let ast = parse_ok(
            r#"forbidden(Order.status == cancelled, Payment.status == captured).along(Order, Payment)"#,
        );
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "forbidden");
        assert_eq!(pred.args.len(), 2);
        assert_eq!(pred.chain.len(), 1);
        assert_eq!(pred.chain[0].name, "along");
        assert_eq!(pred.chain[0].args.len(), 2);
    }

    #[test]
    fn test_parse_multi_chain_invariant() {
        let ast = parse_ok(
            r#"invariant(Order).when(status == delivered).when(refunded == false).then(refund_id == null)"#,
        );
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.chain.len(), 3);
        assert_eq!(pred.chain[0].name, "when");
        assert_eq!(pred.chain[1].name, "when");
        assert_eq!(pred.chain[2].name, "then");
    }

    #[test]
    fn test_parse_typed_qref() {
        let src = r#"
actor    Add "追加"
usecase  Add "追加する"
performs(actor::Add, usecase::Add)
"#;
        let ast = parse_ok(src);
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "performs");
        if let PredicateArg::Ref(qref) = &pred.args[0] {
            assert_eq!(qref.kind_qualifier, Some(Kind::Actor));
            assert_eq!(qref.parts, vec!["Add"]);
        } else {
            panic!("expected Ref arg");
        }
        if let PredicateArg::Ref(qref) = &pred.args[1] {
            assert_eq!(qref.kind_qualifier, Some(Kind::UseCase));
            assert_eq!(qref.parts, vec!["Add"]);
        } else {
            panic!("expected Ref arg");
        }
    }

    // ── 比較式（Expr）のパーステスト ──────────────────────────────────────────

    /// 既存の呼び出し `performs(A, B)` が Expr ではなく Ref になること（後退しないことの確認）
    #[test]
    fn test_existing_call_unaffected_by_expr() {
        let ast = parse_ok("performs(Customer, Browse)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "performs");
        assert!(
            matches!(&pred.args[0], PredicateArg::Ref(_)),
            "first arg should be Ref"
        );
        assert!(
            matches!(&pred.args[1], PredicateArg::Ref(_)),
            "second arg should be Ref"
        );
    }

    #[test]
    fn test_parse_relate_card_and_transition() {
        let relate = parse_ok("relate(Payment, Order, N:1)");
        let pred = relate
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert!(matches!(&pred.args[2], PredicateArg::Card(c) if c == "N:1"));

        let trans = parse_ok("transitions(Order.status, EvCapture, pending -> paid)");
        let pred = trans
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert!(matches!(
            &pred.args[2],
            PredicateArg::Transition { from, to } if from == "pending" && to == "paid"
        ));
    }

    #[test]
    fn test_parse_when_none_quantifier() {
        let ast = parse_ok("when(Cert.status == revoked).none(Assign.status == active)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "when");
        assert_eq!(pred.chain.len(), 1);
        assert_eq!(pred.chain[0].name, "none");
    }

    /// 比較式 `stock < selling` が `PredicateArg::Expr(Cmp)` としてパースされること
    #[test]
    fn test_parse_comparison_col_col() {
        let ast = parse_ok("forbidden(Stock, stock < selling)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "forbidden");
        assert_eq!(pred.args.len(), 2);
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
            assert_eq!(cmp.lhs, Operand::Column("stock".to_string()));
            assert_eq!(cmp.op, CmpOp::Lt);
            assert_eq!(cmp.rhs, Operand::Column("selling".to_string()));
        } else {
            panic!("expected Expr(Cmp), got {:?}", pred.args[1]);
        }
    }

    /// クロスエンティティ比較式 `Order.total > Payment.amount` がパースされること
    #[test]
    fn test_parse_comparison_qualified_columns() {
        let ast = parse_ok("forbidden(Order.total > Payment.amount)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[0] {
            if let Operand::QualifiedColumn(lhs) = &cmp.lhs {
                assert_eq!(lhs.entity.parts, vec!["Order"]);
                assert_eq!(lhs.column, "total");
            } else {
                panic!("expected qualified lhs");
            }
            assert_eq!(cmp.op, CmpOp::Gt);
            if let Operand::QualifiedColumn(rhs) = &cmp.rhs {
                assert_eq!(rhs.entity.parts, vec!["Payment"]);
                assert_eq!(rhs.column, "amount");
            } else {
                panic!("expected qualified rhs");
            }
        } else {
            panic!("expected Expr(Cmp), got {:?}", pred.args[0]);
        }
    }

    /// 比較式 `stock >= 0` (整数リテラル右辺) がパースされること
    #[test]
    fn test_parse_comparison_col_intlit() {
        let ast = parse_ok("forbidden(Stock, stock >= 0)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
            assert_eq!(cmp.lhs, Operand::Column("stock".to_string()));
            assert_eq!(cmp.op, CmpOp::Ge);
            assert_eq!(cmp.rhs, Operand::IntLit("0".to_string()));
        } else {
            panic!("expected Expr(Cmp) with IntLit rhs");
        }
    }

    /// 比較式 `expired_at < now` (組み込み now) がパースされること
    #[test]
    fn test_parse_comparison_col_now() {
        let ast = parse_ok("forbidden(Coupon, expired_at < now)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
            assert_eq!(cmp.lhs, Operand::Column("expired_at".to_string()));
            assert_eq!(cmp.op, CmpOp::Lt);
            assert_eq!(cmp.rhs, Operand::Now);
        } else {
            panic!("expected Expr(Cmp) with Now rhs");
        }
    }

    /// invariant の `.when(expr).then(expr)` 形式
    #[test]
    fn test_parse_invariant_with_comparison_chain() {
        let ast = parse_ok("invariant(Order).when(expired_at < now).then(status == expired)");
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.chain.len(), 2);
        assert_eq!(pred.chain[0].name, "when");
        assert_eq!(pred.chain[0].args.len(), 1);
        assert!(
            matches!(&pred.chain[0].args[0], PredicateArg::Expr(Expr::Cmp(_))),
            "when arg should be Expr(Cmp)"
        );
        assert_eq!(pred.chain[1].name, "then");
        assert_eq!(pred.chain[1].args.len(), 1);
        assert!(matches!(
            &pred.chain[1].args[0],
            PredicateArg::Expr(Expr::Cmp(_))
        ));
    }

    /// 全比較演算子トークンが正しくパースされること
    #[test]
    fn test_parse_all_cmp_ops() {
        let cases = [
            ("a < b", CmpOp::Lt),
            ("a > b", CmpOp::Gt),
            ("a <= b", CmpOp::Le),
            ("a >= b", CmpOp::Ge),
            ("a == b", CmpOp::Eq),
            ("a != b", CmpOp::Ne),
        ];
        for (expr_str, expected_op) in cases {
            let src = format!("forbidden(E, {})", expr_str);
            let ast = parse_ok(&src);
            let pred = ast
                .items
                .iter()
                .find_map(|i| {
                    if let Item::Predicate(p) = i {
                        Some(p)
                    } else {
                        None
                    }
                })
                .expect("predicate not found");
            if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
                assert_eq!(cmp.op, expected_op, "failed for: {}", expr_str);
            } else {
                panic!("expected Expr(Cmp) for: {}", expr_str);
            }
        }
    }

    #[test]
    fn test_parse_property_leads_to() {
        let ast = parse_ok(
            r#"property PaidLeadsToShipped "paid eventually ships"
  leads_to(Order.status == paid, Order.status == shipped)
"#,
        );
        let Item::Property(prop) = &ast.items[0] else {
            panic!("expected Property");
        };
        assert_eq!(prop.id, "PaidLeadsToShipped");
        assert!(matches!(&prop.formula, AstTemporalFormula::LeadsTo { .. }));
    }

    #[test]
    fn test_parse_property_always_with_or() {
        let ast = parse_ok(
            r#"property EventuallyTerminal "terminal"
  eventually(Order.status == delivered or Order.status == cancelled)
"#,
        );
        let Item::Property(prop) = &ast.items[0] else {
            panic!("expected Property");
        };
        assert!(matches!(
            &prop.formula,
            AstTemporalFormula::Eventually(Expr::Or(_, _))
        ));
    }

    #[test]
    fn test_parse_property_tla_or_alias() {
        let ast = parse_ok(
            r#"property EventuallyTerminal "terminal"
  eventually(Order.status == delivered \/ Order.status == cancelled)
"#,
        );
        let Item::Property(prop) = &ast.items[0] else {
            panic!("expected Property");
        };
        assert!(matches!(
            &prop.formula,
            AstTemporalFormula::Eventually(Expr::Or(_, _))
        ));
    }
}
