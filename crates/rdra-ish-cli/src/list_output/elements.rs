//! Structured element list formatters (requirements, APIs, entities, etc.).

use anyhow::Result;

use crate::cli::ListFormat;

use super::node_ref::{adr_targets, node_id, node_kind_name, node_label};
use super::render::{bool_cell, format_rows, optional_cell, repeated_cell};

pub(crate) fn format_requirements(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut requirements: Vec<_> = model.requirements.iter().collect();
    requirements.sort_by_key(|(_, requirement)| requirement.id.as_str());

    let headers = [
        "id",
        "label",
        "priority",
        "sources",
        "stakeholders",
        "owner",
        "acceptance_criteria",
        "status",
        "risk",
        "rationale",
        "description",
    ];
    let rows: Vec<[String; 11]> = requirements
        .into_iter()
        .map(|(_, requirement)| {
            [
                requirement.id.clone(),
                requirement.label.clone(),
                optional_cell(&requirement.priority),
                repeated_cell(&requirement.sources),
                repeated_cell(&requirement.stakeholders),
                optional_cell(&requirement.owner),
                repeated_cell(&requirement.acceptance_criteria),
                optional_cell(&requirement.status),
                optional_cell(&requirement.risk),
                optional_cell(&requirement.rationale),
                optional_cell(&requirement.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "requirements")
}

pub(crate) fn format_adrs(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());

    let headers = [
        "id",
        "label",
        "status",
        "context",
        "decision",
        "consequences",
        "accepted_options",
        "rejected_options",
        "reasons",
        "target_kinds",
        "target_ids",
        "description",
    ];
    let rows: Vec<[String; 12]> = adrs
        .into_iter()
        .map(|(key, adr)| {
            let targets = adr_targets(model, key);
            let target_kinds = targets
                .iter()
                .map(|target| node_kind_name(target).to_string())
                .collect::<Vec<_>>()
                .join("|");
            let target_ids = targets
                .iter()
                .filter_map(|target| node_id(model, target))
                .collect::<Vec<_>>()
                .join("|");
            [
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                repeated_cell(&adr.context),
                optional_cell(&adr.decision),
                repeated_cell(&adr.consequences),
                repeated_cell(&adr.accepted_options),
                repeated_cell(&adr.rejected_options),
                repeated_cell(&adr.reasons),
                target_kinds,
                target_ids,
                optional_cell(&adr.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "ADRs")
}

pub(crate) fn format_adr_impacts(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "adr_id",
        "adr_label",
        "adr_status",
        "target_kind",
        "target_id",
        "target_label",
    ];
    let mut rows = Vec::new();
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());
    for (key, adr) in adrs {
        for target in adr_targets(model, key) {
            rows.push([
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                node_kind_name(&target).to_string(),
                node_id(model, &target).unwrap_or_default(),
                node_label(model, &target).unwrap_or_default(),
            ]);
        }
    }
    rows.sort_by(|left, right| {
        left[0]
            .cmp(&right[0])
            .then_with(|| left[3].cmp(&right[3]))
            .then_with(|| left[4].cmp(&right[4]))
    });

    format_rows(&headers, &rows, format, "ADR impacts")
}

pub(crate) fn format_usecases(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut usecases: Vec<_> = model.use_cases.iter().collect();
    usecases.sort_by_key(|(_, usecase)| usecase.id.as_str());

    let headers = [
        "id",
        "label",
        "preconditions",
        "guards",
        "postconditions",
        "alternatives",
        "errors",
        "description",
    ];
    let rows: Vec<[String; 8]> = usecases
        .into_iter()
        .map(|(_, usecase)| {
            [
                usecase.id.clone(),
                usecase.label.clone(),
                repeated_cell(&usecase.preconditions),
                repeated_cell(&usecase.guards),
                repeated_cell(&usecase.postconditions),
                repeated_cell(&usecase.alternatives),
                repeated_cell(&usecase.errors),
                optional_cell(&usecase.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "use cases")
}

const NFR_HEADERS: [&str; 13] = [
    "id",
    "label",
    "metric",
    "target",
    "window",
    "slo",
    "availability",
    "resilience",
    "audit",
    "logging",
    "retention",
    "privacy",
    "description",
];

fn nfr_row(
    id: &str,
    label: &str,
    description: &Option<String>,
    metadata: [&Option<String>; 10],
) -> [String; 13] {
    [
        id.to_string(),
        label.to_string(),
        optional_cell(metadata[0]),
        optional_cell(metadata[1]),
        optional_cell(metadata[2]),
        optional_cell(metadata[3]),
        optional_cell(metadata[4]),
        optional_cell(metadata[5]),
        optional_cell(metadata[6]),
        optional_cell(metadata[7]),
        optional_cell(metadata[8]),
        optional_cell(metadata[9]),
        optional_cell(description),
    ]
}

pub(crate) fn format_nfrs(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut nfrs: Vec<_> = model.nfrs.iter().collect();
    nfrs.sort_by_key(|(_, nfr)| nfr.id.as_str());

    let rows: Vec<[String; 13]> = nfrs
        .into_iter()
        .map(|(_, nfr)| {
            nfr_row(
                &nfr.id,
                &nfr.label,
                &nfr.description,
                [
                    &nfr.metric,
                    &nfr.target,
                    &nfr.window,
                    &nfr.slo,
                    &nfr.availability,
                    &nfr.resilience,
                    &nfr.audit,
                    &nfr.logging,
                    &nfr.retention,
                    &nfr.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "NFRs")
}

pub(crate) fn format_constraints(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut constraints: Vec<_> = model.constraints.iter().collect();
    constraints.sort_by_key(|(_, constraint)| constraint.id.as_str());

    let rows: Vec<[String; 13]> = constraints
        .into_iter()
        .map(|(_, constraint)| {
            nfr_row(
                &constraint.id,
                &constraint.label,
                &constraint.description,
                [
                    &constraint.metric,
                    &constraint.target,
                    &constraint.window,
                    &constraint.slo,
                    &constraint.availability,
                    &constraint.resilience,
                    &constraint.audit,
                    &constraint.logging,
                    &constraint.retention,
                    &constraint.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "constraints")
}

fn col_type_s(ct: &rdra_ish_core::model::ColumnType) -> &'static str {
    use rdra_ish_core::model::ColumnType;
    match ct {
        ColumnType::Int => "Int",
        ColumnType::String => "String",
        ColumnType::Money => "Money",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Bool => "Bool",
        ColumnType::Decimal => "Decimal",
        ColumnType::Enum(_) => "Enum",
    }
}

pub(crate) fn format_apis(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, api)| api.id.as_str());

    let headers = [
        "api_id",
        "api_label",
        "method",
        "path",
        "idempotency",
        "mode",
        "auth_scheme",
    ];
    let rows: Vec<[String; 7]> = apis
        .into_iter()
        .map(|(_, api)| {
            [
                api.id.clone(),
                api.label.clone(),
                optional_cell(&api.method),
                optional_cell(&api.path),
                optional_cell(&api.idempotency),
                optional_cell(&api.mode),
                optional_cell(&api.auth_scheme),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "APIs")
}

pub(crate) fn format_dtos(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, dto)| dto.id.as_str());

    let headers = [
        "dto_id",
        "dto_label",
        "field_name",
        "field_type",
        "required",
    ];
    let mut rows: Vec<[String; 5]> = Vec::new();
    for (_, dto) in dtos {
        if dto.fields.is_empty() {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for field in &dto.fields {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                field.name.clone(),
                col_type_s(&field.col_type).to_string(),
                bool_cell(!field.is_nullable),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "DTOs")
}

pub(crate) fn format_fields(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut fields: Vec<_> = model.fields.iter().collect();
    fields.sort_by_key(|(_, field)| field.id.as_str());

    let headers = [
        "field_id",
        "field_label",
        "access",
        "required",
        "source",
        "entity_id",
        "column_name",
    ];
    let mut rows: Vec<[String; 7]> = Vec::new();
    for (field_key, field) in fields {
        let mappings: Vec<_> = model
            .field_mappings
            .iter()
            .filter(|mapping| mapping.field == field_key)
            .collect();
        if mappings.is_empty() {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for mapping in mappings {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                model.entities[mapping.entity].id.clone(),
                mapping.column.clone(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "fields")
}

pub(crate) fn format_entities(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut ents: Vec<_> = model.entities.iter().collect();
    ents.sort_by_key(|(_, e)| e.id.as_str());

    let headers = [
        "entity_id",
        "entity_label",
        "column_name",
        "column_type",
        "is_pk",
        "is_unique",
        "is_indexed",
        "is_fk",
        "fk_target",
        "fk_optional",
        "fk_on_delete",
        "fk_on_update",
        "is_nullable",
        "default_val",
        "check_constraints",
        "is_soft_delete",
        "is_history",
        "is_tenant_scope",
        "derived_expr",
    ];

    let mut rows: Vec<[String; 19]> = Vec::new();
    for (_, ent) in &ents {
        for col in &ent.columns {
            rows.push([
                ent.id.clone(),
                ent.label.clone(),
                col.name.clone(),
                col_type_s(&col.col_type).to_string(),
                if col.is_pk { "true" } else { "false" }.to_string(),
                if col.is_unique { "true" } else { "false" }.to_string(),
                if col.is_indexed { "true" } else { "false" }.to_string(),
                if col.is_fk { "true" } else { "false" }.to_string(),
                col.fk_target.clone().unwrap_or_default(),
                bool_cell(col.fk_optional),
                col.fk_on_delete.clone().unwrap_or_default(),
                col.fk_on_update.clone().unwrap_or_default(),
                bool_cell(col.is_nullable),
                col.default_val.clone().unwrap_or_default(),
                col.check_constraints.join("|"),
                bool_cell(col.is_soft_delete),
                bool_cell(col.is_history),
                bool_cell(col.is_tenant_scope),
                col.derived_expr.clone().unwrap_or_default(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "entities")
}
