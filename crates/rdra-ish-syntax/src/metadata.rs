//! Metadata clauses of an instance declaration (`priority must`, `target "<=300ms"`).
//!
//! Every clause takes one value, written either as a quoted string or as a bare token
//! (an identifier, a keyword used as a word, or a number such as `300` or `99.9`). The
//! declaration's kind decides where an ambiguous keyword (`source`) lands.

#![allow(clippy::result_large_err)]

use chumsky::prelude::*;

use crate::ast::{
    AdrMetadata, ApiMetadata, FieldMetadata, Kind, NfrMetadata, RequirementMetadata,
    UseCaseMetadata,
};
use crate::parser::{path_ident, string_lit};
use crate::token::Token;

/// A clause value and whether it was written without quotes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetaValue {
    pub text: String,
    pub bare: bool,
}

/// One quoted string or one bare token.
pub(crate) fn metadata_value() -> impl Parser<Token, MetaValue, Error = Simple<Token>> + Clone {
    let int = select! { Token::IntLit(s) => s };
    let number =
        int.then(just(Token::Dot).ignore_then(int).or_not())
            .map(|(whole, frac)| match frac {
                Some(frac) => format!("{whole}.{frac}"),
                None => whole,
            });
    path_ident()
        .or(number)
        .map(|text| MetaValue { text, bare: true })
        .or(string_lit().map(|text| MetaValue { text, bare: false }))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Slot {
    Priority,
    /// `source`: a requirement's source, or a field's source on a `field`.
    Source,
    Stakeholder,
    Owner,
    AcceptanceCriterion,
    Status,
    Risk,
    Rationale,
    AdrStatus,
    Context,
    Decision,
    Consequence,
    AcceptedOption,
    RejectedOption,
    Reason,
    Method,
    Path,
    Idempotency,
    Mode,
    AuthScheme,
    Metric,
    Baseline,
    Target,
    Window,
    Slo,
    Availability,
    Resilience,
    Audit,
    Logging,
    Retention,
    Privacy,
    Access,
    Required(bool),
    FieldSource,
    Precondition,
    Postcondition,
    Guard,
    Alternative,
    Error,
}

/// Each clause keyword, the keyword `fmt` writes for it, and where its value goes.
/// `required` (a bool) and the removed `acceptance criteria` are parsed separately.
pub(crate) const CLAUSES: &[(&str, &str, Slot)] = &[
    ("priority", "priority", Slot::Priority),
    ("source", "source", Slot::Source),
    ("stakeholder", "stakeholder", Slot::Stakeholder),
    ("owner", "owner", Slot::Owner),
    (
        "acceptance_criteria",
        "acceptance_criteria",
        Slot::AcceptanceCriterion,
    ),
    ("status", "status", Slot::Status),
    ("risk", "risk", Slot::Risk),
    ("rationale", "rationale", Slot::Rationale),
    ("adr_status", "adr_status", Slot::AdrStatus),
    ("context", "context", Slot::Context),
    ("decision", "decision", Slot::Decision),
    ("consequence", "consequence", Slot::Consequence),
    ("accepted_option", "accepted", Slot::AcceptedOption),
    ("accepted", "accepted", Slot::AcceptedOption),
    ("rejected_option", "rejected", Slot::RejectedOption),
    ("rejected", "rejected", Slot::RejectedOption),
    ("reason", "reason", Slot::Reason),
    ("method", "method", Slot::Method),
    ("path", "path", Slot::Path),
    ("idempotency", "idempotency", Slot::Idempotency),
    ("mode", "mode", Slot::Mode),
    ("auth_scheme", "auth", Slot::AuthScheme),
    ("auth", "auth", Slot::AuthScheme),
    ("metric", "metric", Slot::Metric),
    ("baseline", "baseline", Slot::Baseline),
    ("target", "target", Slot::Target),
    ("window", "window", Slot::Window),
    ("slo", "slo", Slot::Slo),
    ("availability", "availability", Slot::Availability),
    ("resilience", "resilience", Slot::Resilience),
    ("audit", "audit", Slot::Audit),
    ("logging", "logging", Slot::Logging),
    ("retention", "retention", Slot::Retention),
    ("privacy_classification", "privacy", Slot::Privacy),
    ("privacy", "privacy", Slot::Privacy),
    ("access", "access", Slot::Access),
    ("input", "source", Slot::FieldSource),
    ("derived", "source", Slot::FieldSource),
    ("precondition", "precondition", Slot::Precondition),
    ("postcondition", "postcondition", Slot::Postcondition),
    ("guard", "guard", Slot::Guard),
    ("alternative_flow", "alternative", Slot::Alternative),
    ("alternative", "alternative", Slot::Alternative),
    ("error_condition", "error", Slot::Error),
    ("business_error", "error", Slot::Error),
    ("error", "error", Slot::Error),
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Clause {
    slot: Slot,
    /// The keyword `fmt` writes for this clause.
    keyword: &'static str,
    value: MetaValue,
}

fn keyword_clause(
    word: &'static str,
    keyword: &'static str,
    slot: Slot,
) -> impl Parser<Token, Clause, Error = Simple<Token>> + Clone {
    just(Token::Ident(word.to_string()))
        .ignore_then(metadata_value())
        .map(move |value| Clause {
            slot,
            keyword,
            value,
        })
}

/// One metadata clause of any kind.
pub(crate) fn metadata_clause() -> impl Parser<Token, Clause, Error = Simple<Token>> + Clone {
    let keywords = choice(
        CLAUSES
            .iter()
            .map(|(word, keyword, slot)| keyword_clause(word, keyword, *slot))
            .collect::<Vec<_>>(),
    );
    let required = just(Token::Ident("required".to_string()))
        .ignore_then(
            just(Token::Ident("true".to_string()))
                .to(true)
                .or(just(Token::Ident("false".to_string())).to(false)),
        )
        .map(|value| Clause {
            slot: Slot::Required(value),
            keyword: "required",
            value: MetaValue {
                text: value.to_string(),
                bare: true,
            },
        });
    // `acceptance criteria "..."` (and `acceptance "..."`) still parse, so the rest of the
    // declaration does, but report the one spelling that remains.
    let acceptance = just(Token::Ident("acceptance".to_string()))
        .ignore_then(just(Token::Ident("criteria".to_string())).or_not())
        .then(metadata_value())
        .validate(|(criteria, value), span, emit| {
            let written = if criteria.is_some() {
                "acceptance criteria"
            } else {
                "acceptance"
            };
            emit(Simple::custom(
                span,
                format!(
                    "`{written}` is no longer supported; write `acceptance_criteria {}`",
                    write_value(&value)
                ),
            ));
            Clause {
                slot: Slot::AcceptanceCriterion,
                keyword: "acceptance_criteria",
                value,
            }
        });
    keywords.or(required).or(acceptance)
}

fn write_value(value: &MetaValue) -> String {
    if value.bare {
        value.text.clone()
    } else {
        format!("\"{}\"", value.text)
    }
}

/// The clauses of one declaration, sorted into the metadata of each kind.
#[derive(Debug, Default)]
pub(crate) struct Metadata {
    pub requirement: RequirementMetadata,
    pub adr: AdrMetadata,
    pub api: ApiMetadata,
    pub nfr: NfrMetadata,
    pub field: FieldMetadata,
    pub usecase: UseCaseMetadata,
    /// `(keyword fmt writes, value)` for each value written without quotes.
    pub bare_values: Vec<(String, String)>,
}

impl Metadata {
    pub(crate) fn collect(kind: &Kind, clauses: Vec<Clause>) -> Self {
        let mut m = Metadata::default();
        for Clause {
            slot,
            keyword,
            value,
        } in clauses
        {
            if value.bare && !matches!(slot, Slot::Required(_)) {
                m.bare_values
                    .push((keyword.to_string(), value.text.clone()));
            }
            let v = value.text;
            match slot {
                Slot::Priority => m.requirement.priority = Some(v),
                Slot::Source if *kind == Kind::Field => m.field.source = Some(v),
                Slot::Source => m.requirement.sources.push(v),
                Slot::Stakeholder => m.requirement.stakeholders.push(v),
                Slot::Owner => m.requirement.owner = Some(v),
                Slot::AcceptanceCriterion => m.requirement.acceptance_criteria.push(v),
                Slot::Status => m.requirement.status = Some(v),
                Slot::Risk => m.requirement.risk = Some(v),
                Slot::Rationale => m.requirement.rationale = Some(v),
                Slot::AdrStatus => m.adr.status = Some(v),
                Slot::Context => m.adr.context.push(v),
                Slot::Decision => m.adr.decision = Some(v),
                Slot::Consequence => m.adr.consequences.push(v),
                Slot::AcceptedOption => m.adr.accepted_options.push(v),
                Slot::RejectedOption => m.adr.rejected_options.push(v),
                Slot::Reason => m.adr.reasons.push(v),
                Slot::Method => m.api.method = Some(v),
                Slot::Path => m.api.path = Some(v),
                Slot::Idempotency => m.api.idempotency = Some(v),
                Slot::Mode => m.api.mode = Some(v),
                Slot::AuthScheme => m.api.auth_scheme = Some(v),
                Slot::Metric => m.nfr.metric = Some(v),
                Slot::Baseline => m.nfr.baseline = Some(v),
                Slot::Target => m.nfr.target = Some(v),
                Slot::Window => m.nfr.window = Some(v),
                Slot::Slo => m.nfr.slo = Some(v),
                Slot::Availability => m.nfr.availability = Some(v),
                Slot::Resilience => m.nfr.resilience = Some(v),
                Slot::Audit => m.nfr.audit = Some(v),
                Slot::Logging => m.nfr.logging = Some(v),
                Slot::Retention => m.nfr.retention = Some(v),
                Slot::Privacy => m.nfr.privacy = Some(v),
                Slot::Access => m.field.access = Some(v),
                Slot::Required(b) => m.field.required = Some(b),
                Slot::FieldSource => m.field.source = Some(v),
                Slot::Precondition => m.usecase.preconditions.push(v),
                Slot::Postcondition => m.usecase.postconditions.push(v),
                Slot::Guard => m.usecase.guards.push(v),
                Slot::Alternative => m.usecase.alternatives.push(v),
                Slot::Error => m.usecase.errors.push(v),
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::{Slot, CLAUSES};
    use crate::format::format_source;
    use crate::parse_error::format_parse_error;
    use crate::parser::parse;

    /// The declaration kind a clause belongs to.
    fn host(slot: Slot) -> &'static str {
        use Slot::*;
        match slot {
            Priority | Source | Stakeholder | Owner | AcceptanceCriterion | Status | Risk
            | Rationale => "requirement",
            AdrStatus | Context | Decision | Consequence | AcceptedOption | RejectedOption
            | Reason => "adr",
            Method | Path | Idempotency | Mode | AuthScheme => "api",
            Baseline => "goal",
            Metric | Target | Window | Slo | Availability | Resilience | Audit | Logging
            | Retention | Privacy => "nfr",
            Access | Required(_) | FieldSource => "field",
            Precondition | Postcondition | Guard | Alternative | Error => "usecase",
        }
    }

    fn errors(src: &str) -> Vec<String> {
        parse(src).1.iter().map(format_parse_error).collect()
    }

    /// Every clause keyword takes a quoted string and a bare token (an identifier or a
    /// number), and `fmt` writes each back the way it was written, under the keyword `fmt`
    /// uses for that clause.
    #[test]
    fn every_clause_takes_a_quoted_string_and_a_bare_token() {
        for (word, keyword, slot) in CLAUSES {
            let kind = host(*slot);
            for value in [
                "\"free text, quoted\"",
                "must",
                "p95_latency",
                "300",
                "99.9",
            ] {
                let src = format!("{kind} X \"X\"\n  {word} {value}\n");
                assert_eq!(errors(&src), Vec::<String>::new(), "{src}");
                let expected = format!("{kind} X \"X\"\n  {keyword} {value}\n");
                assert_eq!(format_source(&src).unwrap(), expected, "{src}");
            }
        }
    }

    #[test]
    fn description_takes_a_quoted_string_and_a_bare_token() {
        for value in ["\"A customer.\"", "customer"] {
            let src = format!("actor X \"X\"\n  description {value}\n");
            assert_eq!(format_source(&src).unwrap(), src);
        }
    }

    #[test]
    fn a_quoted_single_token_stays_quoted() {
        let src = "requirement R \"R\"\n  priority \"must\"\n  status draft\n";
        assert_eq!(format_source(src).unwrap(), src);
    }

    #[test]
    fn required_takes_a_bool() {
        let src = "field F \"F\"\n  required true\n";
        assert_eq!(format_source(src).unwrap(), src);
        assert!(!errors("field F \"F\"\n  required maybe\n").is_empty());
    }

    #[test]
    fn source_on_a_field_is_the_field_source() {
        let (ast, errs) =
            parse("field F \"F\"\n  source actor\nrequirement R \"R\"\n  source interview\n");
        assert!(errs.is_empty(), "{errs:?}");
        let crate::ast::Item::Instance(field) = &ast.items[0] else {
            panic!("field");
        };
        assert_eq!(field.field.source.as_deref(), Some("actor"));
        assert!(field.requirement.sources.is_empty());
        let crate::ast::Item::Instance(req) = &ast.items[1] else {
            panic!("requirement");
        };
        assert_eq!(req.requirement.sources, vec!["interview"]);
    }

    #[test]
    fn acceptance_criteria_in_two_words_names_the_one_spelling() {
        assert_eq!(
            errors("requirement R \"R\"\n  acceptance criteria \"Paid orders ship.\"\n"),
            vec!["`acceptance criteria` is no longer supported; write `acceptance_criteria \"Paid orders ship.\"`"]
        );
        assert_eq!(
            errors("requirement R \"R\"\n  acceptance \"Paid orders ship.\"\n"),
            vec!["`acceptance` is no longer supported; write `acceptance_criteria \"Paid orders ship.\"`"]
        );
    }
}
