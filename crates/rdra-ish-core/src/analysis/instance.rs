//! Instance declaration registration into the semantic model.

use std::collections::HashSet;

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::{DeclSite, DiagCtxt};
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::arg_resolve::resolve_variation;
use super::column_expr::resolve_column_exprs;
use super::metadata::{
    adr_metadata_is_empty, api_metadata_is_empty, field_metadata_is_empty, nfr_metadata_is_empty,
    requirement_metadata_is_empty, usecase_metadata_is_empty,
};

pub(crate) fn register_instance(
    model: &mut SemanticModel,
    inst: &InstanceDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    if inst.kind != Kind::Requirement && !requirement_metadata_is_empty(&inst.requirement) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::RequirementMetadataOnNonRequirement {
                id: inst.id.clone(),
            },
        );
    }
    if inst.kind != Kind::Adr && !adr_metadata_is_empty(&inst.adr) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::AdrMetadataOnNonAdr {
                id: inst.id.clone(),
            },
        );
    }
    if inst.kind != Kind::Api && !api_metadata_is_empty(&inst.api) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::ApiMetadataOnNonApi {
                id: inst.id.clone(),
            },
        );
    }
    if !matches!(inst.kind, Kind::Goal | Kind::Nfr | Kind::Constraint)
        && !nfr_metadata_is_empty(&inst.nfr)
    {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::NfrMetadataOnInvalidKind {
                id: inst.id.clone(),
            },
        );
    }
    if inst.kind != Kind::Field && !field_metadata_is_empty(&inst.field) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::FieldMetadataOnNonField {
                id: inst.id.clone(),
            },
        );
    }
    if inst.kind != Kind::UseCase && !usecase_metadata_is_empty(&inst.usecase) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::UseCaseMetadataOnNonUseCase {
                id: inst.id.clone(),
            },
        );
    }

    let node = match inst.kind {
        Kind::Actor => {
            let k = model.actors.insert(Actor {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Actor(k)
        }
        Kind::ExtSystem => {
            let k = model.ext_systems.insert(ExtSystem {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::ExtSystem(k)
        }
        Kind::System => {
            let k = model.systems.insert(System {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::System(k)
        }
        Kind::Requirement => {
            let k = model.requirements.insert(Requirement {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                priority: inst.requirement.priority.clone(),
                sources: inst.requirement.sources.clone(),
                stakeholders: inst.requirement.stakeholders.clone(),
                owner: inst.requirement.owner.clone(),
                acceptance_criteria: inst.requirement.acceptance_criteria.clone(),
                status: inst.requirement.status.clone(),
                risk: inst.requirement.risk.clone(),
                rationale: inst.requirement.rationale.clone(),
            });
            NodeRef::Requirement(k)
        }
        Kind::Adr => {
            let k = model.adrs.insert(Adr {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                status: inst.adr.status.clone(),
                context: inst.adr.context.clone(),
                decision: inst.adr.decision.clone(),
                consequences: inst.adr.consequences.clone(),
                accepted_options: inst.adr.accepted_options.clone(),
                rejected_options: inst.adr.rejected_options.clone(),
                reasons: inst.adr.reasons.clone(),
            });
            NodeRef::Adr(k)
        }
        Kind::Goal => {
            let k = model.goals.insert(Goal {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                metric: inst.nfr.metric.clone(),
                baseline: inst.nfr.baseline.clone(),
                target: inst.nfr.target.clone(),
                window: inst.nfr.window.clone(),
            });
            NodeRef::Goal(k)
        }
        Kind::Nfr => {
            let k = model.nfrs.insert(Nfr {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                metric: inst.nfr.metric.clone(),
                target: inst.nfr.target.clone(),
                window: inst.nfr.window.clone(),
                slo: inst.nfr.slo.clone(),
                availability: inst.nfr.availability.clone(),
                resilience: inst.nfr.resilience.clone(),
                audit: inst.nfr.audit.clone(),
                logging: inst.nfr.logging.clone(),
                retention: inst.nfr.retention.clone(),
                privacy: inst.nfr.privacy.clone(),
            });
            NodeRef::Nfr(k)
        }
        Kind::Quality => {
            let k = model.qualities.insert(Quality {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Quality(k)
        }
        Kind::Constraint => {
            let k = model.constraints.insert(Constraint {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                metric: inst.nfr.metric.clone(),
                target: inst.nfr.target.clone(),
                window: inst.nfr.window.clone(),
                slo: inst.nfr.slo.clone(),
                availability: inst.nfr.availability.clone(),
                resilience: inst.nfr.resilience.clone(),
                audit: inst.nfr.audit.clone(),
                logging: inst.nfr.logging.clone(),
                retention: inst.nfr.retention.clone(),
                privacy: inst.nfr.privacy.clone(),
            });
            NodeRef::Constraint(k)
        }
        Kind::Business => {
            let k = model.businesses.insert(Business {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Business(k)
        }
        Kind::Buc => {
            let k = model.bucs.insert(Buc {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Buc(k)
        }
        Kind::Flow => {
            let k = model.flows.insert(Flow {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Flow(k)
        }
        Kind::Step => {
            let k = model.steps.insert(Step {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Step(k)
        }
        Kind::UseCase => {
            let k = model.use_cases.insert(UseCase {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                preconditions: inst.usecase.preconditions.clone(),
                postconditions: inst.usecase.postconditions.clone(),
                guards: inst.usecase.guards.clone(),
                alternatives: inst.usecase.alternatives.clone(),
                errors: inst.usecase.errors.clone(),
            });
            NodeRef::UseCase(k)
        }
        Kind::Screen => {
            let k = model.screens.insert(Screen {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Screen(k)
        }
        Kind::Field => {
            let k = model.fields.insert(Field {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                access: inst.field.access.clone(),
                required: inst.field.required,
                source: inst.field.source.clone(),
            });
            NodeRef::Field(k)
        }
        Kind::Event => {
            let k = model.events.insert(Event {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Event(k)
        }
        Kind::Entity => {
            report_duplicate_columns(inst, "entity", ctx, diags);
            let columns = model_columns(model, inst, ctx, diags);
            let primary_key = collect_primary_key(&inst.columns);
            let unique_constraints = collect_unique_constraints(&inst.columns);
            let indexes = collect_indexes(&inst.columns);
            let k = model.entities.insert(Entity {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                columns,
                primary_key,
                unique_constraints,
                indexes,
            });
            apply_primary_key_flags(&mut model.entities[k]);
            NodeRef::Entity(k)
        }
        Kind::Api => {
            let k = model.apis.insert(Api {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                method: inst.api.method.clone(),
                path: inst.api.path.clone(),
                idempotency: inst.api.idempotency.clone(),
                mode: inst.api.mode.clone(),
                auth_scheme: inst.api.auth_scheme.clone(),
            });
            NodeRef::Api(k)
        }
        Kind::Dto => {
            report_duplicate_columns(inst, "dto", ctx, diags);
            let fields = model_columns(model, inst, ctx, diags);
            let k = model.dtos.insert(Dto {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                fields,
            });
            NodeRef::Dto(k)
        }
        Kind::Timer => {
            let k = model.timers.insert(Timer {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Timer(k)
        }
        Kind::Medium => {
            let k = model.media.insert(Medium {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Medium(k)
        }
        Kind::Permission => {
            let k = model.permissions.insert(Permission {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Permission(k)
        }
        Kind::Condition => {
            let k = model.conditions.insert(Condition {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                body: None,
            });
            NodeRef::Condition(k)
        }
        Kind::Variation => {
            let k = model.variations.insert(Variation {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                values: inst
                    .variation_values
                    .iter()
                    .map(|v| v.value.clone())
                    .collect(),
                labels: inst
                    .variation_values
                    .iter()
                    .map(|v| v.label.clone())
                    .collect(),
            });
            NodeRef::Variation(k)
        }
    };

    if model.symbols.insert_in_module(
        inst.id.clone(),
        node,
        model
            .import_scopes
            .module_for_source(ctx.source_id)
            .map(str::to_string),
    ) {
        push_error(
            ctx,
            diags,
            inst.span.clone(),
            RdraError::DuplicateDefinition {
                id: inst.id.clone(),
            },
        );
    } else {
        model.decl_sites.insert(
            inst.kind.name(),
            &inst.id,
            DeclSite {
                source_id: ctx.source_id,
                span: inst.span.clone(),
            },
        );
    }
}

fn model_columns(
    model: &SemanticModel,
    inst: &InstanceDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<ModelColumn> {
    let mut columns: Vec<ModelColumn> = inst
        .columns
        .iter()
        .map(|col| ast_column_to_model(model, inst, col, ctx, diags))
        .collect();
    resolve_column_exprs(inst, &mut columns, ctx, diags);
    columns
}

fn ast_column_to_model(
    model: &SemanticModel,
    inst: &InstanceDecl,
    col: &Column,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> ModelColumn {
    let col_type = match &col.col_type {
        ColType::Int => ColumnType::Int,
        ColType::String => ColumnType::String,
        ColType::Money => ColumnType::Money,
        ColType::DateTime => ColumnType::DateTime,
        ColType::Date => ColumnType::Date,
        ColType::Bool => ColumnType::Bool,
        ColType::Decimal => ColumnType::Decimal,
        ColType::Enum(vs) => ColumnType::Enum(vs.iter().map(|v| v.value.clone()).collect()),
        ColType::Named(name) => match resolve_variation(model, name, ctx) {
            Some(key) => ColumnType::Variation(key),
            None => {
                push_error(
                    ctx,
                    diags,
                    col.span.clone(),
                    RdraError::UnknownColumnType {
                        entity: inst.id.clone(),
                        col: col.name.clone(),
                        name: name.clone(),
                    },
                );
                // Keep the column so later passes still see it; it carries no values.
                ColumnType::Enum(Vec::new())
            }
        },
    };
    let mut mc = ModelColumn {
        name: col.name.clone(),
        col_type,
        is_pk: false,
        is_unique: false,
        is_indexed: false,
        is_nullable: false,
        default_val: None,
        label: None,
        check_constraints: Vec::new(),
        is_soft_delete: false,
        is_history: false,
        is_tenant_scope: false,
        derived_expr: None,
        value_labels: match &col.col_type {
            ColType::Enum(vs) => vs.iter().map(|v| v.label.clone()).collect(),
            _ => Vec::new(),
        },
    };
    for ann in &col.annotations {
        match ann {
            Annotation::Pk => mc.is_pk = true,
            Annotation::PkComposite(_) => {}
            Annotation::Unique => mc.is_unique = true,
            Annotation::UniqueComposite(_) => {}
            Annotation::Index => mc.is_indexed = true,
            Annotation::IndexComposite(_) => {}
            // Resolved against the whole column list by `resolve_column_exprs`.
            Annotation::Check(_) | Annotation::Derived(_) => {}
            Annotation::Null => mc.is_nullable = true,
            Annotation::Default(v) => {
                if let ColumnType::Variation(key) = &mc.col_type {
                    if let Some(variation) = model.variations.get(*key) {
                        if !variation.values.iter().any(|value| value == v) {
                            push_error(
                                ctx,
                                diags,
                                col.span.clone(),
                                RdraError::InvalidVariationValue {
                                    variation: variation.id.clone(),
                                    value: v.clone(),
                                    allowed: variation.values.join(", "),
                                },
                            );
                        }
                    }
                }
                mc.default_val = Some(v.clone());
            }
            Annotation::Label(l) => mc.label = Some(l.clone()),
            Annotation::SoftDelete => mc.is_soft_delete = true,
            Annotation::History => mc.is_history = true,
            Annotation::Tenant => mc.is_tenant_scope = true,
        }
    }
    mc
}

/// Apply composite PK membership onto column flags after entity assembly.
pub(crate) fn apply_primary_key_flags(entity: &mut Entity) {
    if entity.primary_key.is_empty() {
        return;
    }
    for col in &mut entity.columns {
        col.is_pk = entity.primary_key.iter().any(|pk| pk == &col.name);
    }
}

fn collect_primary_key(columns: &[Column]) -> Vec<String> {
    // Prefer an explicit composite `@pk(a, b)` if present.
    for col in columns {
        for ann in &col.annotations {
            if let Annotation::PkComposite(cols) = ann {
                return cols.clone();
            }
        }
    }
    columns
        .iter()
        .filter(|c| c.annotations.iter().any(|a| matches!(a, Annotation::Pk)))
        .map(|c| c.name.clone())
        .collect()
}

fn collect_unique_constraints(columns: &[Column]) -> Vec<Vec<String>> {
    let mut constraints = Vec::new();
    for col in columns {
        for ann in &col.annotations {
            match ann {
                Annotation::Unique => constraints.push(vec![col.name.clone()]),
                Annotation::UniqueComposite(cols) => constraints.push(cols.clone()),
                _ => {}
            }
        }
    }
    constraints
}

fn collect_indexes(columns: &[Column]) -> Vec<Vec<String>> {
    let mut indexes = Vec::new();
    for col in columns {
        for ann in &col.annotations {
            match ann {
                Annotation::Index => indexes.push(vec![col.name.clone()]),
                Annotation::IndexComposite(cols) => indexes.push(cols.clone()),
                _ => {}
            }
        }
    }
    indexes
}

fn report_duplicate_columns(
    inst: &InstanceDecl,
    kind: &str,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let mut seen = HashSet::new();
    for col in &inst.columns {
        if !seen.insert(col.name.as_str()) {
            push_error(
                ctx,
                diags,
                col.span.clone(),
                RdraError::DuplicateColumn {
                    kind: kind.to_string(),
                    id: inst.id.clone(),
                    col: col.name.clone(),
                },
            );
        }
    }
}
