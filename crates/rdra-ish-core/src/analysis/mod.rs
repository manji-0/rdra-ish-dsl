use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::{DeclSite, DiagCtxt, SourceId};
use crate::model::*;
use crate::predicate::predicate_signature;
use rdra_ish_syntax::ast::*;
mod effect;
mod metadata;

use effect::parse_effect_value;
use metadata::{
    adr_metadata_is_empty, api_metadata_is_empty, field_metadata_is_empty, nfr_metadata_is_empty,
    requirement_metadata_is_empty, usecase_metadata_is_empty,
};

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let items: Vec<(SourceId, Item)> = ast.items.iter().cloned().map(|item| (0, item)).collect();
    build_model_items(&items)
}

pub fn build_model_items(items: &[(SourceId, Item)]) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel::default();
    let mut diags: Vec<Diagnostic> = vec![];

    for (source_id, item) in items {
        if let Item::Instance(inst) = item {
            register_instance(&mut model, inst, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    for (source_id, item) in items {
        if let Item::Predicate(pred) = item {
            process_predicate(&mut model, pred, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    generate_fks(&mut model, &mut diags);

    (model, diags)
}

fn register_instance(
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
    if !matches!(inst.kind, Kind::Nfr | Kind::Constraint) && !nfr_metadata_is_empty(&inst.nfr) {
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
        Kind::Concept => {
            let k = model.concepts.insert(Concept {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Concept(k)
        }
        Kind::DomainObject => {
            let k = model.domain_objects.insert(DomainObject {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::DomainObject(k)
        }
        Kind::Aggregate => {
            let k = model.aggregates.insert(Aggregate {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Aggregate(k)
        }
        Kind::ValueObject => {
            let k = model.value_objects.insert(ValueObject {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::ValueObject(k)
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
        Kind::UsageScene => {
            let k = model.usage_scenes.insert(UsageScene {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::UsageScene(k)
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
            let columns = inst.columns.iter().map(ast_column_to_model).collect();
            let unique_constraints = collect_unique_constraints(&inst.columns);
            let indexes = collect_indexes(&inst.columns);
            let k = model.entities.insert(Entity {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                columns,
                unique_constraints,
                indexes,
            });
            NodeRef::Entity(k)
        }
        Kind::State => {
            let k = model.states.insert(State {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::State(k)
        }
        Kind::Condition => {
            let k = model.conditions.insert(Condition {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Condition(k)
        }
        Kind::Variation => {
            let k = model.variations.insert(Variation {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Variation(k)
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
            let fields = inst.columns.iter().map(ast_column_to_model).collect();
            let k = model.dtos.insert(Dto {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                fields,
            });
            NodeRef::Dto(k)
        }
        Kind::Location => {
            let k = model.locations.insert(Location {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Location(k)
        }
        Kind::Timing => {
            let k = model.timings.insert(Timing {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Timing(k)
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
    };

    if model.symbols.insert(inst.id.clone(), node) {
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

fn ast_column_to_model(col: &Column) -> ModelColumn {
    let col_type = match &col.col_type {
        ColType::Int => ColumnType::Int,
        ColType::String => ColumnType::String,
        ColType::Money => ColumnType::Money,
        ColType::DateTime => ColumnType::DateTime,
        ColType::Date => ColumnType::Date,
        ColType::Bool => ColumnType::Bool,
        ColType::Decimal => ColumnType::Decimal,
        ColType::Enum(vs) => ColumnType::Enum(vs.clone()),
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
        is_fk: false,
        fk_target: None,
        fk_optional: false,
        fk_on_delete: None,
        fk_on_update: None,
        check_constraints: Vec::new(),
        is_soft_delete: false,
        is_history: false,
        is_tenant_scope: false,
        derived_expr: None,
    };
    for ann in &col.annotations {
        match ann {
            Annotation::Pk | Annotation::PkComposite(_) => mc.is_pk = true,
            Annotation::Unique => mc.is_unique = true,
            Annotation::UniqueComposite(_) => {}
            Annotation::Index => mc.is_indexed = true,
            Annotation::IndexComposite(_) => {}
            Annotation::Check(expr) => mc.check_constraints.push(expr.clone()),
            Annotation::Null => mc.is_nullable = true,
            Annotation::Default(v) => mc.default_val = Some(v.clone()),
            Annotation::Label(l) => mc.label = Some(l.clone()),
            Annotation::SoftDelete => mc.is_soft_delete = true,
            Annotation::History => mc.is_history = true,
            Annotation::Tenant => mc.is_tenant_scope = true,
            Annotation::Derived(expr) => mc.derived_expr = Some(expr.clone()),
        }
    }
    mc
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

fn resolve_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    match arg {
        PredicateArg::Lit(_) => None,
        PredicateArg::Tuple(_) => None, // タプルはシンボル解決しない
        PredicateArg::Expr(_) => None,  // 比較式はシンボル解決しない
        PredicateArg::Ref(qref) => {
            let id = qref.parts.last().unwrap();

            if let Some(kind) = &qref.kind_qualifier {
                // Kind-qualified: `usecase::Foo` — exact lookup
                model
                    .symbols
                    .lookup_qualified(kind, id)
                    .cloned()
                    .or_else(|| {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::UndefinedSymbol {
                                id: format!("{}::{}", kind.name(), id),
                            },
                        );
                        None
                    })
            } else {
                // Unqualified: `Foo` or `a.Foo`
                match model.symbols.lookup(id) {
                    LookupResult::Found(n) => Some(n.clone()),
                    LookupResult::NotFound => {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::UndefinedSymbol { id: id.clone() },
                        );
                        None
                    }
                    LookupResult::Ambiguous(kinds) => {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::AmbiguousReference {
                                id: id.clone(),
                                kinds: kinds.join(", "),
                            },
                        );
                        None
                    }
                }
            }
        }
    }
}

// ── 制約述語用ヘルパー ────────────────────────────────────────────────────────

/// `Lit(s)` または kind修飾なし1セグメントの `Ref` から文字列を取り出す。
/// `when(status, delivered)` の裸ident引数と `sets(...)` の引用符付きリテラル
/// 引数の両方を許容するための統一抽出。
fn arg_as_str(arg: &PredicateArg) -> Option<String> {
    match arg {
        PredicateArg::Lit(s) => Some(s.clone()),
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some(qref.parts[0].clone())
        }
        _ => None,
    }
}

#[derive(Default)]
struct EntityConditions {
    equals: Vec<(String, EffectValue)>,
    comparisons: Vec<ComparisonProp>,
}

fn resolve_entity_equals_condition(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    column_arg: &PredicateArg,
    value_arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<(String, EffectValue)>, ()> {
    let Some(col_str) = arg_as_str(column_arg) else {
        return Ok(None);
    };
    let Some(val_str) = arg_as_str(value_arg) else {
        return Ok(None);
    };
    let Some(col) = entity_cols.iter().find(|c| c.name == col_str).cloned() else {
        push_error(
            ctx,
            diags,
            arg_span(column_arg),
            RdraError::UnknownColumn {
                entity: entity_id.to_string(),
                col: col_str,
            },
        );
        return Err(());
    };
    match parse_effect_value(&col, &val_str) {
        Ok(value) => Ok(Some((col_str, value))),
        Err(e) => {
            push_error_parse_effect(ctx, diags, value_arg, e);
            Err(())
        }
    }
}

fn collect_entity_conditions(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityConditions> {
    let mut conditions = EntityConditions::default();
    let mut idx = 0;
    while idx < args.len() {
        match &args[idx] {
            PredicateArg::Expr(Expr::Cmp(cmp)) => {
                if let Some(prop) = resolve_comparison(entity_cols, entity_id, cmp, ctx, diags) {
                    conditions.comparisons.push(prop);
                }
                idx += 1;
            }
            PredicateArg::Tuple(elems) if elems.len() == 2 => {
                match resolve_entity_equals_condition(
                    entity_cols,
                    entity_id,
                    &elems[0],
                    &elems[1],
                    ctx,
                    diags,
                ) {
                    Ok(Some(condition)) => conditions.equals.push(condition),
                    Ok(None) => {}
                    Err(()) => return None,
                }
                idx += 1;
            }
            _ if idx + 1 < args.len() => {
                match resolve_entity_equals_condition(
                    entity_cols,
                    entity_id,
                    &args[idx],
                    &args[idx + 1],
                    ctx,
                    diags,
                ) {
                    Ok(Some(condition)) => {
                        conditions.equals.push(condition);
                        idx += 2;
                    }
                    Ok(None) => idx += 1,
                    Err(()) => return None,
                }
            }
            _ => idx += 1,
        }
    }
    Some(conditions)
}

fn context_value_from_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    expected_kind: &str,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<BusinessMappingContextValue> {
    match arg {
        PredicateArg::Lit(s) => Some(BusinessMappingContextValue::Text(s.clone())),
        PredicateArg::Ref(_) => {
            let node = resolve_arg(model, arg, ctx, diags)?;
            let actual = node_kind_tag_str(&node);
            if actual != expected_kind {
                push_error(
                    ctx,
                    diags,
                    arg_span(arg),
                    RdraError::TypeMismatch {
                        pred: "belongs context".to_string(),
                        id: context_arg_id(arg),
                        actual: actual.to_string(),
                        expected: expected_kind.to_string(),
                    },
                );
                return None;
            }
            Some(BusinessMappingContextValue::Ref(node))
        }
        PredicateArg::Tuple(_) | PredicateArg::Expr(_) => None,
    }
}

fn context_arg_id(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Tuple(_) => "<tuple>".to_string(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
    }
}

// ── クロスエンティティ制約ヘルパー ───────────────────────────────────────────

fn qref_id(qref: &QRef) -> Option<String> {
    if qref.parts.len() == 1 {
        Some(qref.parts[0].clone())
    } else {
        None
    }
}

fn qref_display(qref: &QRef) -> String {
    let id = qref.parts.join(".");
    match &qref.kind_qualifier {
        Some(kind) => format!("{}::{}", kind.name(), id),
        None => id,
    }
}

fn qualified_column_display(qcol: &QualifiedColumnRef) -> String {
    format!("{}.{}", qref_display(&qcol.entity), qcol.column)
}

fn push_unique_entity(scope: &mut Vec<EntityKey>, entity: EntityKey) {
    if !scope.contains(&entity) {
        scope.push(entity);
    }
}

fn condition_entities(cond: &CrossEntityCondition, out: &mut Vec<EntityKey>) {
    match cond {
        CrossEntityCondition::Equals { column, .. } => push_unique_entity(out, column.entity),
        CrossEntityCondition::Comparison(prop) => {
            push_unique_entity(out, prop.lhs.entity);
            if let CrossCmpRhs::Column(col) = &prop.rhs {
                push_unique_entity(out, col.entity);
            }
        }
    }
}

fn resolve_entity_qref(
    model: &SemanticModel,
    pred: &str,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    let id = qref.parts.last()?.clone();
    if let Some(kind) = &qref.kind_qualifier {
        if kind != &Kind::Entity {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: qref_display(qref),
                    actual: kind.name().to_string(),
                    expected: "entity".to_string(),
                },
            );
            return None;
        }
        return match model.symbols.lookup_qualified(kind, &id).cloned() {
            Some(NodeRef::Entity(k)) => Some(k),
            _ => {
                push_error_qref(
                    ctx,
                    diags,
                    qref,
                    RdraError::UndefinedSymbol {
                        id: format!("entity::{}", id),
                    },
                );
                None
            }
        };
    }

    if let Some(NodeRef::Entity(k)) = model.symbols.lookup_qualified(&Kind::Entity, &id).cloned() {
        return Some(k);
    }

    match model.symbols.lookup(&id) {
        LookupResult::Found(node) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id,
                    actual: node_kind_tag_str(node).to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        LookupResult::NotFound => {
            push_error_qref(ctx, diags, qref, RdraError::UndefinedSymbol { id });
            None
        }
        LookupResult::Ambiguous(kinds) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::AmbiguousReference {
                    id,
                    kinds: kinds.join(", "),
                },
            );
            None
        }
    }
}

fn resolve_entity_scope_arg(
    model: &SemanticModel,
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    match arg {
        PredicateArg::Ref(qref) => resolve_entity_qref(model, pred, qref, ctx, diags),
        PredicateArg::Lit(s) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: s.clone(),
                    actual: "literal".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Tuple(_) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<tuple>".to_string(),
                    actual: "tuple".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            push_error_cmp(
                ctx,
                diags,
                cmp,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<expr>".to_string(),
                    actual: "expression".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
    }
}

fn split_cross_column_ref(arg: &PredicateArg) -> Option<(Option<QRef>, String)> {
    match arg {
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some((None, qref.parts[0].clone()))
        }
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 2 => {
            let entity = QRef {
                kind_qualifier: None,
                parts: vec![qref.parts[0].clone()],
                span: qref.span.clone(),
            };
            Some((Some(entity), qref.parts[1].clone()))
        }
        _ => None,
    }
}

fn find_entity_column(
    model: &SemanticModel,
    entity: EntityKey,
    column: &str,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<ModelColumn> {
    let entity_id = model.entities[entity].id.clone();
    match model.entities[entity]
        .columns
        .iter()
        .find(|c| c.name == column)
        .cloned()
    {
        Some(col) => Some(col),
        None => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::UnknownColumn {
                    entity: entity_id,
                    col: column.to_string(),
                },
            );
            None
        }
    }
}

fn resolve_cross_column_arg(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(QualifiedModelColumnRef, ModelColumn)> {
    let (entity_ref, column) = split_cross_column_ref(arg)?;
    let entity = match entity_ref {
        Some(qref) => resolve_entity_qref(model, pred, &qref, ctx, diags)?,
        None if scope.len() == 1 => scope[0],
        None => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::CrossConstraintColumnNeedsEntity {
                    column: column.clone(),
                    example: format!("Entity.{}", column),
                },
            );
            return None;
        }
    };
    let model_col = find_entity_column(model, entity, &column, arg_span(arg), ctx, diags)?;
    Some((QualifiedModelColumnRef { entity, column }, model_col))
}

fn resolve_cross_operand_column(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    operand: &Operand,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(QualifiedModelColumnRef, ModelColumn)> {
    match operand {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col = find_entity_column(model, entity, column, span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            ))
        }
        Operand::Column(column) => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::CrossConstraintColumnNeedsEntity {
                    column: column.clone(),
                    example: format!("Entity.{}", column),
                },
            );
            None
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags)?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            ))
        }
        Operand::IntLit(_) | Operand::Now => None,
    }
}

fn resolve_cross_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossComparisonProp> {
    let (lhs, lhs_col) = match resolve_cross_operand_column(
        model,
        scope,
        pred,
        &cmp.lhs,
        cmp.span.clone(),
        ctx,
        diags,
    ) {
        Some(v) => v,
        None => {
            push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
            return None;
        }
    };
    let lhs_cat = type_category(&lhs_col.col_type);

    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        push_error_cmp(
            ctx,
            diags,
            cmp,
            RdraError::ComparisonOpNotOrdered {
                col: lhs.column.clone(),
                col_type: format!("{:?}", lhs_col.col_type),
                op: cmp.op.as_str().to_string(),
            },
        );
        return None;
    }

    let rhs = match &cmp.rhs {
        Operand::Column(_) | Operand::QualifiedColumn(_) => {
            let (rhs_ref, rhs_col) = resolve_cross_operand_column(
                model,
                scope,
                pred,
                &cmp.rhs,
                cmp.span.clone(),
                ctx,
                diags,
            )?;
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_ref.column.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Column(rhs_ref)
        }
        Operand::IntLit(s) => {
            if lhs_cat != "numeric" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: s.clone(),
                        rhs_type: "integer_literal".to_string(),
                    },
                );
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CrossCmpRhs::IntLit(n),
                Err(_) => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                    );
                    return None;
                }
            }
        }
        Operand::Now => {
            if lhs_cat != "temporal" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs.column.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Now
        }
    };

    Some(CrossComparisonProp {
        lhs,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

fn resolve_cross_equals_condition(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    column_arg: &PredicateArg,
    value_arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    let (column, model_col) = resolve_cross_column_arg(model, scope, pred, column_arg, ctx, diags)?;
    let value_lit = arg_as_str(value_arg)?;
    match parse_effect_value(&model_col, &value_lit) {
        Ok(value) => Some(CrossEntityCondition::Equals { column, value }),
        Err(e) => {
            push_error_parse_effect(ctx, diags, value_arg, e);
            None
        }
    }
}

fn resolve_cross_condition(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    match arg {
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            resolve_cross_comparison(model, scope, pred, cmp, ctx, diags)
                .map(CrossEntityCondition::Comparison)
        }
        PredicateArg::Tuple(elems) if elems.len() == 2 => {
            resolve_cross_equals_condition(model, scope, pred, &elems[0], &elems[1], ctx, diags)
        }
        _ => None,
    }
}

fn collect_cross_scope_prefix(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<EntityKey>, usize) {
    let mut scope = Vec::new();
    let mut first_condition = pred.args.len();
    for (idx, arg) in pred.args.iter().enumerate() {
        if matches!(arg, PredicateArg::Tuple(_) | PredicateArg::Expr(_)) {
            first_condition = idx;
            break;
        }
        match arg {
            PredicateArg::Ref(_) => {
                if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
                    push_unique_entity(&mut scope, entity);
                }
            }
            _ => {
                first_condition = idx;
                break;
            }
        }
    }
    (scope, first_condition)
}

fn collect_cross_chain_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match &args[idx] {
            PredicateArg::Expr(_) | PredicateArg::Tuple(_) => {
                if let Some(cond) =
                    resolve_cross_condition(model, scope, pred, &args[idx], ctx, diags)
                {
                    conditions.push(cond);
                }
                idx += 1;
            }
            _ if idx + 1 < args.len() => {
                if let Some(cond) = resolve_cross_equals_condition(
                    model,
                    scope,
                    pred,
                    &args[idx],
                    &args[idx + 1],
                    ctx,
                    diags,
                ) {
                    conditions.push(cond);
                }
                idx += 2;
            }
            _ => {
                idx += 1;
            }
        }
    }
    conditions
}

fn add_condition_entities_to_scope(
    scope: &mut Vec<EntityKey>,
    conditions: &[CrossEntityCondition],
) {
    for cond in conditions {
        condition_entities(cond, scope);
    }
}

fn collect_cross_along_path(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<Vec<EntityKey>> {
    let along = pred.chain.iter().find(|cc| cc.name == "along")?;
    let mut path = Vec::new();
    for arg in &along.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            path.push(entity);
        }
    }
    Some(path)
}

fn cross_scope_semantics_from_chain(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    match collect_cross_along_path(model, pred, ctx, diags) {
        Some(path) => CrossConstraintScope::RelationPath(path),
        None => CrossConstraintScope::GlobalProduct,
    }
}

fn process_cross_forbidden(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (mut scope, first_condition) = collect_cross_scope_prefix(model, pred, ctx, diags);
    let mut conditions = Vec::new();
    for arg in pred.args.iter().skip(first_condition) {
        if let Some(cond) = resolve_cross_condition(model, &scope, &pred.name, arg, ctx, diags) {
            conditions.push(cond);
        }
    }

    if conditions.is_empty() {
        return;
    }

    add_condition_entities_to_scope(&mut scope, &conditions);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    model
        .cross_forbidden_constraints
        .push(CrossForbiddenConstraint {
            scope: scope.clone(),
            scope_semantics,
            conditions,
        });
    model
        .typed_predicates
        .push(TypedPredicate::CrossForbidden { scope });
}

fn process_cross_invariant(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let mut scope = Vec::new();
    for arg in &pred.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            push_unique_entity(&mut scope, entity);
        }
    }

    let mut guards = Vec::new();
    let mut requireds = Vec::new();
    for cc in &pred.chain {
        match cc.name.as_str() {
            "when" => guards.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "then" => requireds.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "has" | "none" => process_quantifier_chain(model, &scope, &guards, cc, ctx, diags),
            _ => {}
        }
    }

    if guards.is_empty() || requireds.is_empty() {
        return;
    }

    add_condition_entities_to_scope(&mut scope, &guards);
    add_condition_entities_to_scope(&mut scope, &requireds);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    model.cross_entity_invariants.push(CrossEntityInvariant {
        scope: scope.clone(),
        scope_semantics,
        guards,
        requireds,
    });
    model
        .typed_predicates
        .push(TypedPredicate::CrossInvariant { scope });
}

fn process_forbidden_when_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::Entity(anchor))) = resolved.first() else {
        return;
    };
    let scope = vec![*anchor];
    let guards =
        collect_cross_chain_conditions(model, &scope, &pred.name, &pred.args[1..], ctx, diags);
    if guards.is_empty() {
        return;
    }

    for cc in &pred.chain {
        if matches!(cc.name.as_str(), "has" | "none") {
            process_quantifier_chain(model, &scope, &guards, cc, ctx, diags);
        }
    }
    model
        .typed_predicates
        .push(TypedPredicate::ForbiddenWhen { entity: *anchor });
}

fn process_quantifier_chain(
    model: &mut SemanticModel,
    anchor_scope: &[EntityKey],
    guards: &[CrossEntityCondition],
    cc: &ChainCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(anchor) = anchor_scope.first().copied() else {
        return;
    };
    let Some(related_arg) = cc.args.first() else {
        return;
    };
    let Some(related) = resolve_entity_scope_arg(model, &cc.name, related_arg, ctx, diags) else {
        return;
    };
    let related_scope = vec![related];
    let related_conditions =
        collect_cross_chain_conditions(model, &related_scope, &cc.name, &cc.args[1..], ctx, diags);
    if related_conditions.is_empty() {
        return;
    }

    let kind = match cc.name.as_str() {
        "has" => crate::model::QuantifierKind::Has,
        "none" => crate::model::QuantifierKind::None,
        _ => return,
    };
    model
        .quantifier_constraints
        .push(crate::model::QuantifierConstraint {
            anchor,
            guards: guards.to_vec(),
            kind,
            related,
            related_conditions,
        });
}

fn temporal_equals_from_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    if cmp.op != CmpOp::Eq {
        return None;
    }

    let (column, model_col) = match &cmp.lhs {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col =
                find_entity_column(model, entity, column, cmp.span.clone(), ctx, diags)?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            )
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags)?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            )
        }
        _ => return None,
    };

    let value_lit = match &cmp.rhs {
        Operand::Column(value) | Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) => return None,
    };
    match parse_effect_value(&model_col, &value_lit) {
        Ok(value) => Some(CrossEntityCondition::Equals { column, value }),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            None
        }
    }
}

fn collect_temporal_assert_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match &args[idx] {
            PredicateArg::Expr(Expr::Cmp(cmp)) => {
                if let Some(cond) =
                    temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags)
                {
                    conditions.push(cond);
                } else if let Some(cond) =
                    resolve_cross_condition(model, scope, pred, &args[idx], ctx, diags)
                {
                    conditions.push(cond);
                }
                idx += 1;
            }
            PredicateArg::Tuple(_) => {
                if let Some(cond) =
                    resolve_cross_condition(model, scope, pred, &args[idx], ctx, diags)
                {
                    conditions.push(cond);
                }
                idx += 1;
            }
            _ if idx + 1 < args.len() => {
                if let Some(cond) = resolve_cross_equals_condition(
                    model,
                    scope,
                    pred,
                    &args[idx],
                    &args[idx + 1],
                    ctx,
                    diags,
                ) {
                    conditions.push(cond);
                }
                idx += 2;
            }
            _ => idx += 1,
        }
    }
    conditions
}

fn process_after_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::UseCase(anchor))) = resolved.first() else {
        return;
    };

    let mut scope = Vec::new();
    let mut requireds = Vec::new();
    for cc in &pred.chain {
        if cc.name == "assert" {
            requireds.extend(collect_temporal_assert_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            ));
        }
    }
    if requireds.is_empty() {
        return;
    }

    add_condition_entities_to_scope(&mut scope, &requireds);
    model
        .temporal_assertions
        .push(crate::model::TemporalAssertion {
            anchor: *anchor,
            scope,
            requireds,
        });
    model
        .typed_predicates
        .push(TypedPredicate::After { anchor: *anchor });
}

// ── 比較式の型整合チェック・モデル変換 ────────────────────────────────────────

/// `ColumnType` が「比較に使える型カテゴリ」を返す。
/// - `"numeric"`: Int/Money/Decimal
/// - `"temporal"`: Date/DateTime
/// - `"equality"`: それ以外（等値比較 == / != のみ許容）
/// - `"none"`: 比較不可（比較を拒否）
fn type_category(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int | ColumnType::Money | ColumnType::Decimal => "numeric",
        ColumnType::Date | ColumnType::DateTime => "temporal",
        ColumnType::String | ColumnType::Bool | ColumnType::Enum(_) => "equality",
    }
}

/// `CmpOp` が順序比較か（`<`, `>`, `<=`, `>=`）。
fn is_order_op(op: &CmpOp) -> bool {
    matches!(op, CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge)
}

/// `ast::CmpOp` → `model::CmpOpModel` への変換。
fn to_model_op(op: &CmpOp) -> CmpOpModel {
    match op {
        CmpOp::Lt => CmpOpModel::Lt,
        CmpOp::Gt => CmpOpModel::Gt,
        CmpOp::Le => CmpOpModel::Le,
        CmpOp::Ge => CmpOpModel::Ge,
        CmpOp::Eq => CmpOpModel::Eq,
        CmpOp::Ne => CmpOpModel::Ne,
    }
}

/// 比較式 `Comparison` を解析して `ComparisonProp` に変換する。
///
/// - 左辺はカラム参照必須。
/// - 演算子と右辺の型整合を検査する。
/// - 型不整合があれば `diags` にエラーを push し `None` を返す。
fn resolve_comparison(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<ComparisonProp> {
    // ── 左辺はカラム参照のみ ──────────────────────────────────────────────────
    let lhs_col_name = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = qref_id(&qcol.entity) else {
                push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
                return None;
            };
            if q_entity != entity_id {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::TypeMismatch {
                        pred: "comparison".to_string(),
                        id: qualified_column_display(qcol),
                        actual: format!("column of entity {}", q_entity),
                        expected: format!("column of entity {}", entity_id),
                    },
                );
                return None;
            }
            qcol.column.clone()
        }
        _ => {
            push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
            return None;
        }
    };

    // 左辺カラムを解決
    let lhs_col = match entity_cols.iter().find(|c| c.name == lhs_col_name) {
        Some(c) => c,
        None => {
            push_error_cmp(
                ctx,
                diags,
                cmp,
                RdraError::UnknownColumn {
                    entity: entity_id.to_string(),
                    col: lhs_col_name.clone(),
                },
            );
            return None;
        }
    };

    let lhs_cat = type_category(&lhs_col.col_type);

    // 順序比較演算子が使えない型か確認
    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        push_error_cmp(
            ctx,
            diags,
            cmp,
            RdraError::ComparisonOpNotOrdered {
                col: lhs_col_name.clone(),
                col_type: format!("{:?}", lhs_col.col_type),
                op: cmp.op.as_str().to_string(),
            },
        );
        return None;
    }

    // ── 右辺の解決と型整合チェック ────────────────────────────────────────────
    let rhs = match &cmp.rhs {
        Operand::Column(rhs_name) => {
            let rhs_col = match entity_cols.iter().find(|c| &c.name == rhs_name) {
                Some(c) => c,
                None => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonRhsColumnUnknown {
                            entity: entity_id.to_string(),
                            col: rhs_name.clone(),
                        },
                    );
                    return None;
                }
            };
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_name.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Column(rhs_name.clone())
        }
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = qref_id(&qcol.entity) else {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonRhsColumnUnknown {
                        entity: entity_id.to_string(),
                        col: qualified_column_display(qcol),
                    },
                );
                return None;
            };
            if q_entity != entity_id {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::TypeMismatch {
                        pred: "comparison".to_string(),
                        id: qualified_column_display(qcol),
                        actual: format!("column of entity {}", q_entity),
                        expected: format!("column of entity {}", entity_id),
                    },
                );
                return None;
            }
            let rhs_name = qcol.column.clone();
            let rhs_col = match entity_cols.iter().find(|c| c.name == rhs_name) {
                Some(c) => c,
                None => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonRhsColumnUnknown {
                            entity: entity_id.to_string(),
                            col: rhs_name.clone(),
                        },
                    );
                    return None;
                }
            };
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_name.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Column(rhs_name)
        }
        Operand::IntLit(s) => {
            if lhs_cat != "numeric" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: s.clone(),
                        rhs_type: "integer_literal".to_string(),
                    },
                );
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CmpRhs::IntLit(n),
                Err(_) => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                    );
                    return None;
                }
            }
        }
        Operand::Now => {
            if lhs_cat != "temporal" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs_col_name.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Now
        }
    };

    Some(ComparisonProp {
        lhs_column: lhs_col_name,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

fn resolve_predicate_args(
    model: &SemanticModel,
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<NodeRef>> {
    pred.args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let Some(kinds) = sig.get(i) else {
                return if matches!(pred.name.as_str(), "forbidden" | "required" | "exclusive") {
                    None
                } else {
                    resolve_arg(model, arg, ctx, diags)
                };
            };
            if matches!(kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
                return None;
            }
            resolve_arg(model, arg, ctx, diags)
        })
        .collect()
}

fn predicate_arg_display(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Tuple(_) => "<tuple>".to_string(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
    }
}

fn validate_predicate_arg_types(
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, expected_kinds) in sig.iter().enumerate() {
        if matches!(expected_kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
            continue;
        }
        if let Some(Some(node)) = resolved.get(i) {
            let actual = node_kind_tag_str(node);
            if !expected_kinds.contains(&actual) {
                push_error_arg(
                    ctx,
                    diags,
                    &pred.args,
                    i,
                    RdraError::TypeMismatch {
                        pred: pred.name.clone(),
                        id: predicate_arg_display(&pred.args[i]),
                        actual: actual.to_string(),
                        expected: expected_kinds.join("|"),
                    },
                );
            }
        }
    }
}

fn validate_contains_pair(
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if pred.name != "contains" {
        return true;
    }
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        let valid = matches!(
            (from, to),
            (NodeRef::Buc(_), NodeRef::UseCase(_))
                | (NodeRef::Buc(_), NodeRef::Flow(_))
                | (NodeRef::Flow(_), NodeRef::Step(_))
                | (NodeRef::Screen(_), NodeRef::Field(_))
                | (NodeRef::System(_), NodeRef::Api(_))
                | (NodeRef::Aggregate(_), NodeRef::DomainObject(_))
                | (NodeRef::Aggregate(_), NodeRef::ValueObject(_))
                | (NodeRef::Aggregate(_), NodeRef::Concept(_))
        );
        if !valid {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::TypeMismatch {
                    pred: pred.name.clone(),
                    id: "contains pair".to_string(),
                    actual: format!("{} -> {}", node_kind_tag_str(from), node_kind_tag_str(to)),
                    expected: "buc->usecase|buc->flow|flow->step|screen->field|system->api|aggregate->domain_object|aggregate->valueobject|aggregate->concept".to_string(),
                },
            );
            return false;
        }
    }
    true
}

fn process_maps_field_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (Some(Some(NodeRef::Field(field))), Some(Some(NodeRef::Entity(entity)))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    let Some(column) = pred.args.get(2).and_then(arg_as_str) else {
        return;
    };

    let entity_id = model.entities[*entity].id.clone();
    if !model.entities[*entity]
        .columns
        .iter()
        .any(|col| col.name == column)
    {
        let span = pred
            .args
            .get(2)
            .map(arg_span)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| pred.span.clone());
        push_error(
            ctx,
            diags,
            span,
            RdraError::UnknownColumn {
                entity: entity_id,
                col: column,
            },
        );
        return;
    }

    model.field_mappings.push(FieldMapping {
        field: *field,
        entity: *entity,
        column,
    });
    model.relations.push(Relation {
        from: NodeRef::Field(*field),
        to: NodeRef::Entity(*entity),
        kind: RelKind::MapsField,
        options: RelationOptions::default(),
    });
}

fn process_coordinates_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (Some(Some(usecase)), Some(Some(left)), Some(Some(right))) =
        (resolved.first(), resolved.get(1), resolved.get(2))
    {
        if let (NodeRef::UseCase(uk), NodeRef::Entity(left_ek), NodeRef::Entity(right_ek)) =
            (usecase, left, right)
        {
            model
                .boundary_coordinations
                .push(crate::model::BoundaryCoordination {
                    usecase: *uk,
                    left: *left_ek,
                    right: *right_ek,
                });
        }
    }
}

fn process_maps_to_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (Some(Some(from)), Some(Some(NodeRef::Entity(entity)))) =
        (resolved.first(), resolved.get(1))
    {
        let Some(source) = ConceptualRef::from_node_ref(from) else {
            return;
        };
        model.concept_mappings.push(ConceptMapping {
            source: source.clone(),
            entity: *entity,
        });
        model.relations.push(Relation {
            from: from.clone(),
            to: NodeRef::Entity(*entity),
            kind: RelKind::MapsTo,
            options: RelationOptions::default(),
        });
    }
}

fn process_transitions_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (
        Some(Some(NodeRef::Event(event))),
        Some(Some(NodeRef::State(state_before))),
        Some(Some(NodeRef::State(state_after))),
    ) = (resolved.first(), resolved.get(1), resolved.get(2))
    {
        model.state_transitions.push(crate::model::StateTransition {
            event: *event,
            from: *state_before,
            to: *state_after,
        });
        model.relations.push(Relation {
            from: NodeRef::State(*state_before),
            to: NodeRef::State(*state_after),
            kind: RelKind::Transitions,
            options: RelationOptions::default(),
        });
    }
}

fn process_outbox_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let Some(Some(NodeRef::Event(event))) = resolved.first() {
        model.outbox_events.insert(*event);
    }
}

fn process_sets_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (Some(Some(origin)), Some(Some(entity_ref))) = (resolved.first(), resolved.get(1)) else {
        return;
    };
    let entity_key = match entity_ref {
        NodeRef::Entity(k) => *k,
        _ => return,
    };

    match pred.args.get(2) {
        Some(PredicateArg::Expr(Expr::Cmp(cmp))) => {
            let truth_str = match pred.args.get(3) {
                Some(PredicateArg::Ref(q)) if q.kind_qualifier.is_none() && q.parts.len() == 1 => {
                    q.parts[0].as_str().to_string()
                }
                Some(PredicateArg::Lit(s)) => s.clone(),
                _ => return,
            };
            if truth_str != "true" && truth_str != "false" {
                return;
            }
            let entity_id = model.entities[entity_key].id.clone();
            let entity_cols = model.entities[entity_key].columns.clone();
            if let Some(prop) = resolve_comparison(&entity_cols, &entity_id, cmp, ctx, diags) {
                model.proposition_effects.push(PropositionEffect {
                    origin: origin.clone(),
                    entity: entity_key,
                    prop: prop.clone(),
                    truth: truth_str == "true",
                });
                if let Some(origin) = DataOrigin::from_node_ref(origin) {
                    model
                        .typed_predicates
                        .push(TypedPredicate::SetsProposition {
                            origin,
                            entity: entity_key,
                            prop,
                            truth: truth_str == "true",
                        });
                }
            }
        }
        Some(PredicateArg::Lit(col_name)) => {
            let col_name = col_name.clone();
            let val_lit = match pred.args.get(3) {
                Some(PredicateArg::Lit(s)) => s.clone(),
                _ => return,
            };
            let col = model.entities[entity_key]
                .columns
                .iter()
                .find(|c| c.name == col_name)
                .cloned();
            let Some(col) = col else {
                push_error(
                    ctx,
                    diags,
                    pred.span.clone(),
                    RdraError::UnknownColumn {
                        entity: model.entities[entity_key].id.clone(),
                        col: col_name,
                    },
                );
                return;
            };
            match parse_effect_value(&col, &val_lit) {
                Ok(value) => {
                    model.column_effects.push(ColumnEffect {
                        origin: origin.clone(),
                        entity: entity_key,
                        column: col_name.clone(),
                        value,
                    });
                    if let Some(origin) = DataOrigin::from_node_ref(origin) {
                        model.typed_predicates.push(TypedPredicate::SetsColumn {
                            origin,
                            entity: entity_key,
                            column: col_name,
                        });
                    }
                }
                Err(e) => {
                    let span = pred
                        .args
                        .get(3)
                        .map(arg_span)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| pred.span.clone());
                    push_error(ctx, diags, span, e);
                }
            }
        }
        _ => {}
    }
}

fn process_forbidden_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.forbidden_constraints.push(ForbiddenConstraint {
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Forbidden { entity: entity_key });
    }
}

fn process_required_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.required_constraints.push(RequiredConstraint {
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Required { entity: entity_key });
    }
}

fn process_exclusive_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if conditions.equals.len() + conditions.comparisons.len() >= 2 {
        model.exclusive_constraints.push(ExclusiveConstraint {
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Exclusive { entity: entity_key });
    }
}

fn process_invariant_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let mut guards: Vec<(String, EffectValue)> = Vec::new();
    let mut guard_comparisons: Vec<ComparisonProp> = Vec::new();
    let mut requireds: Vec<(String, EffectValue)> = Vec::new();
    let mut required_comparisons: Vec<ComparisonProp> = Vec::new();

    for cc in &pred.chain {
        let is_guard = cc.name == "when";
        let is_required = cc.name == "then";
        if !is_guard && !is_required {
            continue;
        }

        let mut processed_eq = false;
        for arg in &cc.args {
            if processed_eq {
                break;
            }
            match arg {
                PredicateArg::Expr(Expr::Cmp(cmp)) => {
                    if let Some(prop) =
                        resolve_comparison(&entity_cols, &entity_id, cmp, ctx, diags)
                    {
                        if is_guard {
                            guard_comparisons.push(prop);
                        } else {
                            required_comparisons.push(prop);
                        }
                    }
                }
                _ => {
                    if cc.args.len() < 2 {
                        break;
                    }
                    let Some(col_str) = arg_as_str(&cc.args[0]) else {
                        break;
                    };
                    let Some(val_str) = arg_as_str(&cc.args[1]) else {
                        break;
                    };
                    let col = entity_cols.iter().find(|c| c.name == col_str).cloned();
                    let Some(col) = col else {
                        push_error(
                            ctx,
                            diags,
                            arg_span(&cc.args[0]),
                            RdraError::UnknownColumn {
                                entity: entity_id.clone(),
                                col: col_str,
                            },
                        );
                        return;
                    };
                    match parse_effect_value(&col, &val_str) {
                        Ok(value) => {
                            if is_guard {
                                guards.push((col_str, value));
                            } else {
                                requireds.push((col_str, value));
                            }
                        }
                        Err(e) => {
                            push_error_parse_effect(ctx, diags, &cc.args[1], e);
                            return;
                        }
                    }
                    processed_eq = true;
                }
            }
        }
    }

    let has_guards = !guards.is_empty() || !guard_comparisons.is_empty();
    let has_requireds = !requireds.is_empty() || !required_comparisons.is_empty();
    if has_guards && has_requireds {
        model.entity_invariants.push(EntityInvariant {
            entity: entity_key,
            guards,
            guard_comparisons,
            requireds,
            required_comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Invariant { entity: entity_key });
    }
}

fn relation_kind_for_predicate(pred_name: &str) -> Option<RelKind> {
    let kind = match pred_name {
        "performs" => RelKind::Performs,
        "uses" => RelKind::Uses,
        "reads" => RelKind::Reads,
        "writes" => RelKind::Writes,
        "creates" => RelKind::Creates,
        "updates" => RelKind::Updates,
        "deletes" => RelKind::Deletes,
        "displays" => RelKind::Displays,
        "shows" => RelKind::Shows,
        "raises" => RelKind::Raises,
        "triggers" => RelKind::Triggers,
        "contains" => RelKind::Contains,
        "belongs" => RelKind::Belongs,
        "has_permission" => RelKind::HasPermission,
        "requires_permission" => RelKind::RequiresPermission,
        "requires_medium" => RelKind::RequiresMedium,
        "motivates" => RelKind::Motivates,
        "decides" => RelKind::Decides,
        "invokes" => RelKind::Invokes,
        "precedes" => RelKind::Precedes,
        "branches" => RelKind::Branches,
        "excepts" => RelKind::Excepts,
        "repeats" => RelKind::Repeats,
        "covers" => RelKind::Covers,
        "compensates" => RelKind::Compensates,
        "request" => RelKind::Request,
        "response" => RelKind::Response,
        "error_response" => RelKind::ErrorResponse,
        "applies_to" => RelKind::AppliesTo,
        "qualifies" => RelKind::Qualifies,
        "constrains" => RelKind::Constrains,
        "maps_to" => RelKind::MapsTo,
        "maps_field" => RelKind::MapsField,
        "owns" => RelKind::Owns,
        _ => return None,
    };
    Some(kind)
}

fn process_belongs_context(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    from: &NodeRef,
    to: &NodeRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (NodeRef::Buc(buc), NodeRef::Business(business)) = (from, to) else {
        return;
    };
    let mut whens = Vec::new();
    let mut wheres = Vec::new();
    let mut bys = Vec::new();

    for cc in &pred.chain {
        let (target, expected_kind) = match cc.name.as_str() {
            "when" => (&mut whens, "timing"),
            "where" => (&mut wheres, "location"),
            "by" => (&mut bys, "medium"),
            _ => continue,
        };
        for arg in &cc.args {
            if let Some(value) = context_value_from_arg(model, arg, expected_kind, ctx, diags) {
                target.push(value);
            }
        }
    }

    if !whens.is_empty() || !wheres.is_empty() || !bys.is_empty() {
        model
            .business_mapping_contexts
            .push(BusinessMappingContext {
                buc: *buc,
                business: *business,
                whens,
                wheres,
                bys,
            });
    }
}

fn process_relation_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(kind) = relation_kind_for_predicate(&pred.name) else {
        return;
    };
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        model.relations.push(Relation {
            from: from.clone(),
            to: to.clone(),
            kind,
            options: RelationOptions::default(),
        });
        if pred.name == "belongs" {
            process_belongs_context(model, pred, from, to, ctx, diags);
        }
    }
}

fn relation_options_from_chain(pred: &PredicateCall) -> RelationOptions {
    let mut options = RelationOptions::default();
    for cc in &pred.chain {
        match cc.name.as_str() {
            "optional" => options.optional = true,
            "on_delete" => {
                if let Some(value) = cc.args.first().and_then(arg_as_str) {
                    options.on_delete = Some(value);
                }
            }
            "on_update" => {
                if let Some(value) = cc.args.first().and_then(arg_as_str) {
                    options.on_update = Some(value);
                }
            }
            _ => {}
        }
    }
    options
}

fn process_relate_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    if let (Some(Some(from)), Some(Some(to)), Some(PredicateArg::Lit(card))) =
        (resolved.first(), resolved.get(1), pred.args.get(2))
    {
        let kind = match card.as_str() {
            "1:1" => RelKind::RelateOneToOne,
            "1:N" => RelKind::RelateOneToMany,
            "N:1" => RelKind::RelateManyToOne,
            "N:M" => {
                let from_id = match from {
                    NodeRef::Entity(k) => model.entities[*k].id.clone(),
                    _ => "?".into(),
                };
                let to_id = match to {
                    NodeRef::Entity(k) => model.entities[*k].id.clone(),
                    _ => "?".into(),
                };
                push_warning(
                    ctx,
                    diags,
                    pred.args
                        .get(2)
                        .map(arg_span)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| pred.span.clone()),
                    RdraError::NMRelation {
                        from: from_id,
                        to: to_id,
                    },
                );
                RelKind::RelateManyToMany
            }
            _ => return,
        };
        model.relations.push(Relation {
            from: from.clone(),
            to: to.clone(),
            kind,
            options: relation_options_from_chain(pred),
        });
    }
}

fn process_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(sig) = predicate_signature(&pred.name) else {
        return;
    };

    match pred.name.as_str() {
        "cross_forbidden" => {
            process_cross_forbidden(model, pred, ctx, diags);
            return;
        }
        "cross_invariant" => {
            process_cross_invariant(model, pred, ctx, diags);
            return;
        }
        _ => {}
    }

    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    validate_predicate_arg_types(pred, &sig, &resolved, ctx, diags);
    if !validate_contains_pair(pred, &resolved, ctx, diags) {
        return;
    }

    match pred.name.as_str() {
        "coordinates" => process_coordinates_predicate(model, &resolved),
        "maps_to" => process_maps_to_predicate(model, &resolved),
        "transitions" => process_transitions_predicate(model, &resolved),
        "outbox" => process_outbox_predicate(model, &resolved),
        "after" => process_after_predicate(model, pred, &resolved, ctx, diags),
        "sets" => process_sets_predicate(model, pred, &resolved, ctx, diags),
        "forbidden" => process_forbidden_predicate(model, pred, &resolved, ctx, diags),
        "invariant" => process_invariant_predicate(model, pred, &resolved, ctx, diags),
        "required" => process_required_predicate(model, pred, &resolved, ctx, diags),
        "exclusive" => process_exclusive_predicate(model, pred, &resolved, ctx, diags),
        "forbidden_when" => process_forbidden_when_predicate(model, pred, &resolved, ctx, diags),
        "maps_field" => process_maps_field_predicate(model, pred, &resolved, ctx, diags),
        "relate" => process_relate_predicate(model, pred, &resolved, ctx, diags),
        _ => process_relation_predicate(model, pred, &resolved, ctx, diags),
    }

    if let Some(typed) = crate::typed_predicate::build_typed_predicate(&pred.name, &resolved, pred)
    {
        model.typed_predicates.push(typed);
    }
}

fn node_kind_tag_str(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Adr(_) => "adr",
        NodeRef::Nfr(_) => "nfr",
        NodeRef::Quality(_) => "quality",
        NodeRef::Constraint(_) => "constraint",
        NodeRef::Concept(_) => "concept",
        NodeRef::DomainObject(_) => "domain_object",
        NodeRef::Aggregate(_) => "aggregate",
        NodeRef::ValueObject(_) => "valueobject",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::Flow(_) => "flow",
        NodeRef::Step(_) => "step",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Field(_) => "field",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Dto(_) => "dto",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}

fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let rels: Vec<_> = model
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelKind::RelateManyToOne | RelKind::RelateOneToMany))
        .map(|r| {
            (
                r.from.clone(),
                r.to.clone(),
                r.kind.clone(),
                r.options.clone(),
            )
        })
        .collect();

    for (from, to, kind, options) in rels {
        let (many_key, one_key) = match kind {
            RelKind::RelateManyToOne => {
                if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (&from, &to) {
                    (*fk, *tk)
                } else {
                    continue;
                }
            }
            RelKind::RelateOneToMany => {
                if let (NodeRef::Entity(ok), NodeRef::Entity(mk)) = (&from, &to) {
                    (*mk, *ok)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let (one_id, pk_type) = {
            let one = &model.entities[one_key];
            let pk = one.columns.iter().find(|c| c.is_pk);
            match pk {
                Some(col) => (one.id.clone(), col.col_type.clone()),
                None => {
                    push_entity_error(
                        model,
                        diags,
                        &one.id,
                        RdraError::MissingPk {
                            entity: one.id.clone(),
                        },
                    );
                    continue;
                }
            }
        };

        let fk_col_name = format!("{}_id", one_id.to_lowercase());

        let many_entity_id = model.entities[many_key].id.clone();
        if model.entities[many_key]
            .columns
            .iter()
            .any(|c| c.name == fk_col_name)
        {
            push_entity_error(
                model,
                diags,
                &many_entity_id,
                RdraError::FkConflict {
                    entity: many_entity_id.clone(),
                    col: fk_col_name.clone(),
                },
            );
            continue;
        }

        let fk_col = ModelColumn {
            name: fk_col_name,
            col_type: pk_type,
            is_pk: false,
            is_unique: false,
            is_indexed: false,
            is_nullable: options.optional,
            default_val: None,
            label: None,
            is_fk: true,
            fk_target: Some(one_id),
            fk_optional: options.optional,
            fk_on_delete: options.on_delete,
            fk_on_update: options.on_update,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
        };
        model.entities[many_key].columns.push(fk_col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_syntax::parse;

    fn instance(kind: Kind, id: &str) -> InstanceDecl {
        InstanceDecl {
            kind,
            id: id.to_string(),
            label: format!("{id} label"),
            description: Some(format!("{id} description")),
            requirement: RequirementMetadata::default(),
            adr: AdrMetadata::default(),
            api: ApiMetadata::default(),
            nfr: NfrMetadata::default(),
            field: FieldMetadata::default(),
            usecase: UseCaseMetadata::default(),
            columns: Vec::new(),
            span: 0..0,
        }
    }

    fn model_column(name: &str, col_type: ColumnType) -> ModelColumn {
        ModelColumn {
            name: name.to_string(),
            col_type,
            is_pk: false,
            is_unique: false,
            is_indexed: false,
            is_nullable: false,
            default_val: None,
            label: None,
            is_fk: false,
            fk_target: None,
            fk_optional: false,
            fk_on_delete: None,
            fk_on_update: None,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
        }
    }

    fn qref(id: &str) -> QRef {
        QRef {
            kind_qualifier: None,
            parts: vec![id.to_string()],
            span: 0..0,
        }
    }

    fn qcol(entity: &str, column: &str) -> Operand {
        Operand::QualifiedColumn(QualifiedColumnRef {
            entity: qref(entity),
            column: column.to_string(),
            span: 0..0,
        })
    }

    fn entity_key(model: &SemanticModel, id: &str) -> EntityKey {
        model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == id).then_some(key))
            .unwrap()
    }

    fn simple_entity_model(ids: &[&str]) -> SemanticModel {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();
        for id in ids {
            let inst = InstanceDecl {
                kind: Kind::Entity,
                id: (*id).to_string(),
                label: format!("{id} label"),
                description: None,
                requirement: RequirementMetadata::default(),
                adr: AdrMetadata::default(),
                api: ApiMetadata::default(),
                nfr: NfrMetadata::default(),
                field: FieldMetadata::default(),
                usecase: UseCaseMetadata::default(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        col_type: ColType::Int,
                        annotations: vec![Annotation::Pk],
                        span: 0..0,
                    },
                    Column {
                        name: "status".to_string(),
                        col_type: ColType::Enum(vec!["open".to_string(), "closed".to_string()]),
                        annotations: Vec::new(),
                        span: 0..0,
                    },
                    Column {
                        name: "amount".to_string(),
                        col_type: ColType::Decimal,
                        annotations: Vec::new(),
                        span: 0..0,
                    },
                ],
                span: 0..0,
            };
            register_instance(&mut model, &inst, DiagCtxt::new(0), &mut diags);
        }
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        model
    }

    #[test]
    fn entity_block_comment_does_not_drop_following_columns() {
        let src = r#"
usecase ActivateExample "Activate example"

entity Example "Example" {
  id: Int @pk
  // Comment between columns should not end the entity body.
  status: Enum(active, inactive)
}

sets(ActivateExample, Example, "status", "active")
"#;

        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "model errors: {errors:?}");

        let example = model
            .entities
            .iter()
            .find_map(|(_, entity)| (entity.id == "Example").then_some(entity))
            .expect("Example entity should be registered");

        let column_names: Vec<_> = example
            .columns
            .iter()
            .map(|col| col.name.as_str())
            .collect();
        assert_eq!(column_names, vec!["id", "status"]);
    }

    #[test]
    fn type_category_groups_comparison_compatible_column_types() {
        assert_eq!(type_category(&ColumnType::Int), "numeric");
        assert_eq!(type_category(&ColumnType::Money), "numeric");
        assert_eq!(type_category(&ColumnType::Decimal), "numeric");
        assert_eq!(type_category(&ColumnType::Date), "temporal");
        assert_eq!(type_category(&ColumnType::DateTime), "temporal");
        assert_eq!(type_category(&ColumnType::String), "equality");
        assert_eq!(type_category(&ColumnType::Bool), "equality");
        assert_eq!(
            type_category(&ColumnType::Enum(vec!["open".to_string()])),
            "equality"
        );
    }

    #[test]
    fn to_model_op_maps_every_ast_comparison_operator() {
        assert_eq!(to_model_op(&CmpOp::Lt), CmpOpModel::Lt);
        assert_eq!(to_model_op(&CmpOp::Gt), CmpOpModel::Gt);
        assert_eq!(to_model_op(&CmpOp::Le), CmpOpModel::Le);
        assert_eq!(to_model_op(&CmpOp::Ge), CmpOpModel::Ge);
        assert_eq!(to_model_op(&CmpOp::Eq), CmpOpModel::Eq);
        assert_eq!(to_model_op(&CmpOp::Ne), CmpOpModel::Ne);
    }

    #[test]
    fn node_kind_tag_str_labels_each_node_ref_kind() {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();
        let cases = [
            (Kind::Actor, "ActorA", "actor"),
            (Kind::ExtSystem, "ExtA", "extsystem"),
            (Kind::System, "SystemA", "system"),
            (Kind::Requirement, "ReqA", "requirement"),
            (Kind::Adr, "AdrA", "adr"),
            (Kind::Nfr, "NfrA", "nfr"),
            (Kind::Quality, "QualityA", "quality"),
            (Kind::Constraint, "ConstraintA", "constraint"),
            (Kind::Concept, "ConceptA", "concept"),
            (Kind::DomainObject, "DomainObjectA", "domain_object"),
            (Kind::Aggregate, "AggregateA", "aggregate"),
            (Kind::ValueObject, "ValueObjectA", "valueobject"),
            (Kind::Business, "BusinessA", "business"),
            (Kind::Buc, "BucA", "buc"),
            (Kind::Flow, "FlowA", "flow"),
            (Kind::Step, "StepA", "step"),
            (Kind::UsageScene, "SceneA", "usagescene"),
            (Kind::UseCase, "UsecaseA", "usecase"),
            (Kind::Screen, "ScreenA", "screen"),
            (Kind::Field, "FieldA", "field"),
            (Kind::Event, "EventA", "event"),
            (Kind::State, "StateA", "state"),
            (Kind::Condition, "ConditionA", "condition"),
            (Kind::Variation, "VariationA", "variation"),
            (Kind::Api, "ApiA", "api"),
            (Kind::Dto, "DtoA", "dto"),
            (Kind::Location, "LocationA", "location"),
            (Kind::Timing, "TimingA", "timing"),
            (Kind::Medium, "MediumA", "medium"),
            (Kind::Permission, "PermissionA", "permission"),
        ];

        for (kind, id, _) in &cases {
            register_instance(
                &mut model,
                &instance(kind.clone(), id),
                DiagCtxt::new(0),
                &mut diags,
            );
        }
        let entity_inst = InstanceDecl {
            kind: Kind::Entity,
            id: "EntityA".to_string(),
            label: "EntityA label".to_string(),
            description: None,
            requirement: RequirementMetadata::default(),
            adr: AdrMetadata::default(),
            api: ApiMetadata::default(),
            nfr: NfrMetadata::default(),
            field: FieldMetadata::default(),
            usecase: UseCaseMetadata::default(),
            columns: vec![Column {
                name: "id".to_string(),
                col_type: ColType::Int,
                annotations: vec![Annotation::Pk],
                span: 0..0,
            }],
            span: 0..0,
        };
        register_instance(&mut model, &entity_inst, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        for (kind, id, expected) in &cases {
            let node = model.symbols.lookup_qualified(kind, id).unwrap();
            assert_eq!(node_kind_tag_str(node), *expected);
        }
        let entity = model
            .symbols
            .lookup_qualified(&Kind::Entity, "EntityA")
            .unwrap();
        assert_eq!(node_kind_tag_str(entity), "entity");
    }

    #[test]
    fn push_unique_entity_preserves_first_seen_scope_order() {
        let model = simple_entity_model(&["Order", "Payment"]);
        let order = entity_key(&model, "Order");
        let payment = entity_key(&model, "Payment");
        let mut scope = Vec::new();

        push_unique_entity(&mut scope, order);
        push_unique_entity(&mut scope, payment);
        push_unique_entity(&mut scope, order);

        assert_eq!(scope, vec![order, payment]);
    }

    #[test]
    fn add_condition_entities_to_scope_adds_equals_and_comparison_entities_once() {
        let model = simple_entity_model(&["Order", "Payment", "Invoice"]);
        let order = entity_key(&model, "Order");
        let payment = entity_key(&model, "Payment");
        let invoice = entity_key(&model, "Invoice");
        let conditions = vec![
            CrossEntityCondition::Equals {
                column: QualifiedModelColumnRef {
                    entity: order,
                    column: "status".to_string(),
                },
                value: EffectValue::EnumVariant("closed".to_string()),
            },
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs: QualifiedModelColumnRef {
                    entity: payment,
                    column: "amount".to_string(),
                },
                op: CmpOpModel::Gt,
                rhs: CrossCmpRhs::Column(QualifiedModelColumnRef {
                    entity: invoice,
                    column: "amount".to_string(),
                }),
            }),
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs: QualifiedModelColumnRef {
                    entity: order,
                    column: "amount".to_string(),
                },
                op: CmpOpModel::Ge,
                rhs: CrossCmpRhs::IntLit(1),
            }),
        ];
        let mut scope = vec![order];

        add_condition_entities_to_scope(&mut scope, &conditions);

        assert_eq!(scope, vec![order, payment, invoice]);
    }

    #[test]
    fn cross_scope_semantics_from_chain_returns_relation_path_for_along_chain() {
        let model = simple_entity_model(&["Order", "Payment"]);
        let mut diags = Vec::new();
        let pred = PredicateCall {
            name: "cross_invariant".to_string(),
            args: Vec::new(),
            chain: vec![ChainCall {
                name: "along".to_string(),
                args: vec![
                    PredicateArg::Ref(qref("Order")),
                    PredicateArg::Ref(qref("Payment")),
                ],
                span: 0..0,
            }],
            span: 0..0,
        };

        let semantics =
            cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let CrossConstraintScope::RelationPath(path) = semantics else {
            panic!("expected relation path scope");
        };
        assert_eq!(
            path,
            vec![entity_key(&model, "Order"), entity_key(&model, "Payment")]
        );
    }

    #[test]
    fn cross_scope_semantics_from_chain_defaults_to_global_product_without_along() {
        let model = simple_entity_model(&["Order"]);
        let mut diags = Vec::new();
        let pred = PredicateCall {
            name: "cross_forbidden".to_string(),
            args: Vec::new(),
            chain: Vec::new(),
            span: 0..0,
        };

        let semantics =
            cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert!(matches!(semantics, CrossConstraintScope::GlobalProduct));
    }

    #[test]
    fn register_instance_populates_each_node_store_and_symbol_table() {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();

        let cases = [
            (Kind::Actor, "ActorA"),
            (Kind::ExtSystem, "ExtA"),
            (Kind::System, "SystemA"),
            (Kind::Requirement, "ReqA"),
            (Kind::Adr, "AdrA"),
            (Kind::Nfr, "NfrA"),
            (Kind::Quality, "QualityA"),
            (Kind::Constraint, "ConstraintA"),
            (Kind::Concept, "ConceptA"),
            (Kind::DomainObject, "DomainObjectA"),
            (Kind::Aggregate, "AggregateA"),
            (Kind::ValueObject, "ValueObjectA"),
            (Kind::Business, "BusinessA"),
            (Kind::Buc, "BucA"),
            (Kind::Flow, "FlowA"),
            (Kind::Step, "StepA"),
            (Kind::UsageScene, "SceneA"),
            (Kind::UseCase, "UsecaseA"),
            (Kind::Screen, "ScreenA"),
            (Kind::Field, "FieldA"),
            (Kind::Event, "EventA"),
            (Kind::State, "StateA"),
            (Kind::Condition, "ConditionA"),
            (Kind::Variation, "VariationA"),
            (Kind::Api, "ApiA"),
            (Kind::Dto, "DtoA"),
            (Kind::Location, "LocationA"),
            (Kind::Timing, "TimingA"),
            (Kind::Medium, "MediumA"),
            (Kind::Permission, "PermissionA"),
        ];

        for (kind, id) in &cases {
            register_instance(
                &mut model,
                &instance(kind.clone(), id),
                DiagCtxt::new(0),
                &mut diags,
            );
        }

        let entity_inst = InstanceDecl {
            kind: Kind::Entity,
            id: "EntityA".to_string(),
            label: "EntityA label".to_string(),
            description: Some("EntityA description".to_string()),
            requirement: RequirementMetadata::default(),
            adr: AdrMetadata::default(),
            api: ApiMetadata::default(),
            nfr: NfrMetadata::default(),
            field: FieldMetadata::default(),
            usecase: UseCaseMetadata::default(),
            columns: vec![Column {
                name: "id".to_string(),
                col_type: ColType::Int,
                annotations: vec![Annotation::Pk],
                span: 0..0,
            }],
            span: 0..0,
        };
        register_instance(&mut model, &entity_inst, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert_eq!(model.actors.len(), 1);
        assert_eq!(model.ext_systems.len(), 1);
        assert_eq!(model.systems.len(), 1);
        assert_eq!(model.requirements.len(), 1);
        assert_eq!(model.nfrs.len(), 1);
        assert_eq!(model.qualities.len(), 1);
        assert_eq!(model.constraints.len(), 1);
        assert_eq!(model.concepts.len(), 1);
        assert_eq!(model.domain_objects.len(), 1);
        assert_eq!(model.aggregates.len(), 1);
        assert_eq!(model.value_objects.len(), 1);
        assert_eq!(model.businesses.len(), 1);
        assert_eq!(model.bucs.len(), 1);
        assert_eq!(model.usage_scenes.len(), 1);
        assert_eq!(model.use_cases.len(), 1);
        assert_eq!(model.screens.len(), 1);
        assert_eq!(model.fields.len(), 1);
        assert_eq!(model.events.len(), 1);
        assert_eq!(model.entities.len(), 1);
        assert_eq!(model.states.len(), 1);
        assert_eq!(model.conditions.len(), 1);
        assert_eq!(model.variations.len(), 1);
        assert_eq!(model.apis.len(), 1);
        assert_eq!(model.dtos.len(), 1);
        assert_eq!(model.locations.len(), 1);
        assert_eq!(model.timings.len(), 1);
        assert_eq!(model.media.len(), 1);
        assert_eq!(model.permissions.len(), 1);

        let entity = model.entities.values().next().unwrap();
        assert_eq!(entity.columns.len(), 1);
        assert!(entity.columns[0].is_pk);

        for (kind, id) in &cases {
            assert!(
                model.symbols.lookup_qualified(kind, id).is_some(),
                "{id} should be present in symbol table"
            );
        }
        assert!(model
            .symbols
            .lookup_qualified(&Kind::Entity, "EntityA")
            .is_some());
    }

    #[test]
    fn build_model_registers_screen_fields_and_column_mappings() {
        let src = r#"
screen CheckoutScreen "Checkout screen"
field ShippingAddress "Shipping address" access editable required true source actor
field OrderTotal "Order total" access readonly required true source system
entity Order "Order" {
  id: Int @pk
  shipping_address: String
  total: Money
}
contains(CheckoutScreen, ShippingAddress)
contains(CheckoutScreen, OrderTotal)
maps_field(ShippingAddress, Order, "shipping_address")
maps_field(OrderTotal, Order, "total")
"#;

        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        assert_eq!(model.fields.len(), 2);
        let shipping = model
            .fields
            .iter()
            .find_map(|(_, field)| (field.id == "ShippingAddress").then_some(field))
            .expect("ShippingAddress field should be registered");
        assert_eq!(shipping.access.as_deref(), Some("editable"));
        assert_eq!(shipping.required, Some(true));
        assert_eq!(shipping.source.as_deref(), Some("actor"));
        assert_eq!(model.field_mappings.len(), 2);
        assert!(model
            .relations
            .iter()
            .any(|rel| matches!(rel.kind, RelKind::MapsField)));
    }

    #[test]
    fn register_instance_reports_duplicate_same_kind_but_keeps_cross_kind_names() {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();

        register_instance(
            &mut model,
            &instance(Kind::Actor, "Same"),
            DiagCtxt::new(0),
            &mut diags,
        );
        register_instance(
            &mut model,
            &instance(Kind::UseCase, "Same"),
            DiagCtxt::new(0),
            &mut diags,
        );
        register_instance(
            &mut model,
            &instance(Kind::Actor, "Same"),
            DiagCtxt::new(0),
            &mut diags,
        );

        assert_eq!(model.actors.len(), 2);
        assert_eq!(model.use_cases.len(), 1);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].error.to_string().contains("duplicate definition"));
        assert!(model
            .symbols
            .lookup_qualified(&Kind::Actor, "Same")
            .is_some());
        assert!(model
            .symbols
            .lookup_qualified(&Kind::UseCase, "Same")
            .is_some());
    }

    #[test]
    fn resolve_comparison_accepts_same_entity_qualified_columns_and_literals() {
        let cols = vec![
            model_column("stock", ColumnType::Int),
            model_column("selling", ColumnType::Int),
            model_column("expired_at", ColumnType::DateTime),
        ];
        let mut diags = Vec::new();

        let col_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: qcol("Stock", "stock"),
                op: CmpOp::Lt,
                rhs: qcol("Stock", "selling"),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();
        let int_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("stock".to_string()),
                op: CmpOp::Ge,
                rhs: Operand::IntLit("10".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();
        let now_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("expired_at".to_string()),
                op: CmpOp::Lt,
                rhs: Operand::Now,
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert_eq!(col_prop.axis_key(), "stock<selling");
        assert_eq!(int_prop.rhs, CmpRhs::IntLit(10));
        assert_eq!(now_prop.rhs, CmpRhs::Now);
    }

    #[test]
    fn resolve_comparison_rejects_cross_entity_and_invalid_type_comparisons() {
        let cols = vec![
            model_column("stock", ColumnType::Int),
            model_column("active", ColumnType::Bool),
            model_column("name", ColumnType::String),
        ];
        let mut diags = Vec::new();

        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: qcol("Other", "stock"),
                op: CmpOp::Lt,
                rhs: Operand::Column("stock".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());
        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("active".to_string()),
                op: CmpOp::Lt,
                rhs: Operand::Column("stock".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());
        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("name".to_string()),
                op: CmpOp::Eq,
                rhs: Operand::IntLit("1".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());

        let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages.iter().any(|msg| msg.contains("type mismatch")),
            "expected cross-entity type mismatch, got {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("order comparison operator")),
            "expected ordered comparison diagnostic, got {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("comparison type mismatch")),
            "expected rhs type mismatch diagnostic, got {messages:?}"
        );
    }

    #[test]
    fn test_build_model_basic() {
        let src = r#"
actor Customer "顧客" description "商品を購入する顧客"
entity Order "注文" description "受注情報" { id: Int @pk }
entity Customer_profile "顧客情報" { id: Int @pk  name: String }
usecase Browse "商品を探す" description "商品一覧を参照する"
performs(Customer, Browse)
relate(Order, Customer_profile, "N:1")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);

        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        assert_eq!(model.actors.len(), 1);
        let actor = model.actors.values().next().unwrap();
        assert_eq!(actor.id, "Customer");
        assert_eq!(actor.label, "顧客");
        assert_eq!(actor.description.as_deref(), Some("商品を購入する顧客"));
        let use_case = model.use_cases.values().next().unwrap();
        assert_eq!(use_case.description.as_deref(), Some("商品一覧を参照する"));

        let order = model
            .entities
            .values()
            .find(|e| e.id == "Order")
            .expect("Order entity not found");
        assert_eq!(order.description.as_deref(), Some("受注情報"));

        let fk_col = order
            .columns
            .iter()
            .find(|c| c.name == "customer_profile_id")
            .expect("customer_profile_id FK column not found");

        assert!(fk_col.is_fk);
        assert_eq!(fk_col.fk_target.as_deref(), Some("Customer_profile"));
        assert_eq!(fk_col.col_type, ColumnType::Int);
    }

    #[test]
    fn test_build_model_requirement_metadata() {
        let src = r#"
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority "must"
  source "Customer interview"
  source "Incident review"
  stakeholder "Store Operations"
  owner "Product Owner"
  acceptance criteria "A payment timeout leaves the cart recoverable."
  status "proposed"
  risk "high"
  rationale "Checkout failures directly block revenue."
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let requirement = model.requirements.values().next().unwrap();
        assert_eq!(requirement.priority.as_deref(), Some("must"));
        assert_eq!(
            requirement.sources,
            vec![
                "Customer interview".to_string(),
                "Incident review".to_string()
            ]
        );
        assert_eq!(
            requirement.stakeholders,
            vec!["Store Operations".to_string()]
        );
        assert_eq!(requirement.owner.as_deref(), Some("Product Owner"));
        assert_eq!(
            requirement.acceptance_criteria,
            vec!["A payment timeout leaves the cart recoverable.".to_string()]
        );
        assert_eq!(requirement.status.as_deref(), Some("proposed"));
        assert_eq!(requirement.risk.as_deref(), Some("high"));
        assert_eq!(
            requirement.rationale.as_deref(),
            Some("Checkout failures directly block revenue.")
        );
    }

    #[test]
    fn test_build_model_adr_metadata_and_decision_links() {
        let src = r#"
adr AdrOutbox "Use transactional outbox"
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."
system CustomerSystem "Customer System"
entity Customer "Customer" { id: Int @pk }
api PublishCustomerChanged "Publish customer changed"
decides(AdrOutbox, CustomerSystem)
decides(AdrOutbox, Customer)
decides(AdrOutbox, PublishCustomerChanged)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let (adr_key, adr) = model.adrs.iter().next().unwrap();
        assert_eq!(adr.status.as_deref(), Some("accepted"));
        assert_eq!(
            adr.decision.as_deref(),
            Some("Publish customer changes through a transactional outbox.")
        );
        assert_eq!(adr.accepted_options, vec!["Transactional outbox"]);
        assert_eq!(adr.rejected_options, vec!["Synchronous callback"]);
        assert_eq!(adr.reasons.len(), 1);

        let target_kinds: Vec<_> = model
            .relations
            .iter()
            .filter(|relation| {
                relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr_key)
            })
            .map(|relation| node_kind_tag_str(&relation.to))
            .collect();
        assert_eq!(target_kinds, vec!["system", "entity", "api"]);
    }

    #[test]
    fn test_build_model_business_flow_relations() {
        let src = r#"
buc BucCheckout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
step AuthorizePayment "Authorize payment"
step PaymentFailed "Payment failed"
usecase CapturePayment "Capture payment"
api PaymentApi "Payment API"
event PaymentRejected "Payment rejected"
contains(BucCheckout, CheckoutFlow)
contains(CheckoutFlow, ReviewCart)
contains(CheckoutFlow, AuthorizePayment)
precedes(ReviewCart, AuthorizePayment)
branches(ReviewCart, PaymentFailed)
excepts(AuthorizePayment, PaymentFailed)
repeats(PaymentFailed, ReviewCart)
covers(AuthorizePayment, CapturePayment)
covers(AuthorizePayment, PaymentApi)
covers(PaymentFailed, PaymentRejected)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.flows.len(), 1);
        assert_eq!(model.steps.len(), 3);
        for kind in [
            RelKind::Precedes,
            RelKind::Branches,
            RelKind::Excepts,
            RelKind::Repeats,
            RelKind::Covers,
        ] {
            assert!(
                model.relations.iter().any(|rel| rel.kind == kind),
                "missing {kind:?} relation"
            );
        }
    }

    #[test]
    fn test_build_model_usecase_metadata_and_compensation() {
        let src = r#"
usecase CapturePayment "Capture payment"
  precondition "Order is authorized."
  guard "Provider is available."
  postcondition "Payment is captured."
  alternative "Customer changes payment method."
  error "Authorization expires."
usecase RefundPayment "Refund payment"
compensates(RefundPayment, CapturePayment)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let capture = model
            .use_cases
            .values()
            .find(|usecase| usecase.id == "CapturePayment")
            .unwrap();
        assert_eq!(capture.preconditions, vec!["Order is authorized."]);
        assert_eq!(capture.guards, vec!["Provider is available."]);
        assert_eq!(capture.postconditions, vec!["Payment is captured."]);
        assert_eq!(
            capture.alternatives,
            vec!["Customer changes payment method."]
        );
        assert_eq!(capture.errors, vec!["Authorization expires."]);
        assert!(
            model
                .relations
                .iter()
                .any(|relation| relation.kind == RelKind::Compensates),
            "compensates should become a relation"
        );
    }

    #[test]
    fn test_build_model_api_contract_metadata_and_dto_relations() {
        let src = r#"
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
dto OrderResponse "Order response" { order_id: Int }
dto ErrorResponse "Error response" { code: String  message: String }
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let api = model.apis.values().next().unwrap();
        assert_eq!(api.method.as_deref(), Some("POST"));
        assert_eq!(api.path.as_deref(), Some("/orders"));
        assert_eq!(api.idempotency.as_deref(), Some("idempotent"));
        assert_eq!(api.mode.as_deref(), Some("sync"));
        assert_eq!(api.auth_scheme.as_deref(), Some("bearer"));

        let request_dto = model
            .dtos
            .values()
            .find(|dto| dto.id == "CreateOrderRequest")
            .unwrap();
        assert_eq!(request_dto.fields.len(), 2);
        assert!(request_dto
            .fields
            .iter()
            .any(|field| field.name == "note" && field.is_nullable));

        for kind in [RelKind::Request, RelKind::Response, RelKind::ErrorResponse] {
            assert!(
                model.relations.iter().any(|rel| rel.kind == kind),
                "missing {kind:?} relation"
            );
        }
    }

    #[test]
    fn test_build_model_non_functional_elements_and_relations() {
        let src = r#"
system CoreSystem "Core system"
usecase Checkout "Checkout"
api CheckoutApi "Checkout API"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable
quality Performance "Performance"
quality Availability "Availability"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
applies_to(CheckoutLatency, Checkout)
applies_to(CheckoutLatency, CheckoutApi)
applies_to(CheckoutLatency, CoreSystem)
qualifies(CheckoutLatency, Performance)
qualifies(AuditRetention, Availability)
constrains(AuditRetention, CoreSystem)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let nfr = model.nfrs.values().next().unwrap();
        assert_eq!(nfr.metric.as_deref(), Some("p95_latency_ms"));
        assert_eq!(nfr.target.as_deref(), Some("<=300"));
        assert_eq!(nfr.window.as_deref(), Some("5m"));
        assert_eq!(nfr.slo.as_deref(), Some("99.9%"));
        assert_eq!(nfr.availability.as_deref(), Some("multi_az"));
        assert_eq!(nfr.resilience.as_deref(), Some("retryable"));

        let constraint = model.constraints.values().next().unwrap();
        assert_eq!(constraint.audit.as_deref(), Some("enabled"));
        assert_eq!(constraint.logging.as_deref(), Some("structured"));
        assert_eq!(constraint.retention.as_deref(), Some("7y"));
        assert_eq!(constraint.privacy.as_deref(), Some("restricted"));

        for kind in [RelKind::AppliesTo, RelKind::Qualifies, RelKind::Constrains] {
            assert!(
                model.relations.iter().any(|rel| rel.kind == kind),
                "missing {kind:?} relation"
            );
        }
    }

    #[test]
    fn test_build_model_system_ownership_relation() {
        let src = r#"
system StoreSystem "Store system"
entity Store "Store" { id: Int @pk }
owns(StoreSystem, Store)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert!(
            model.relations.iter().any(|rel| rel.kind == RelKind::Owns),
            "owns(System, Entity) should become an Owns relation"
        );
    }

    #[test]
    fn test_build_model_conceptual_elements_and_entity_mapping() {
        let src = r#"
concept PatientIdentity "Patient identity"
concept CarePlan "Care plan"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"
entity AppointmentTable "appointment table" { id: Int @pk  starts_at: DateTime }
contains(SchedulingAggregate, Appointment)
contains(SchedulingAggregate, TimeSlot)
contains(SchedulingAggregate, PatientIdentity)
maps_to(Appointment, AppointmentTable)
maps_to(TimeSlot, AppointmentTable)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.concepts.len(), 2);
        assert_eq!(model.domain_objects.len(), 1);
        assert_eq!(model.aggregates.len(), 1);
        assert_eq!(model.value_objects.len(), 1);

        assert!(model
            .concepts
            .values()
            .any(|concept| concept.id == "CarePlan"));
        assert_eq!(model.entities.len(), 1);

        let contains_count = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Contains)
            .count();
        let maps_to_count = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::MapsTo)
            .count();
        assert_eq!(contains_count, 3);
        assert_eq!(maps_to_count, 2);
        assert_eq!(model.concept_mappings.len(), 2);
        assert!(model.concept_mappings.iter().any(|mapping| {
            matches!(
                (&mapping.source, model.entities[mapping.entity].id.as_str()),
                (ConceptualRef::DomainObject(_), "AppointmentTable")
            ) && model.domain_objects.values().any(|d| d.id == "Appointment")
        }));
        assert!(model.concept_mappings.iter().any(|mapping| {
            matches!(
                (&mapping.source, model.entities[mapping.entity].id.as_str()),
                (ConceptualRef::ValueObject(_), "AppointmentTable")
            ) && model.value_objects.values().any(|v| v.id == "TimeSlot")
        }));
    }

    #[test]
    fn test_build_model_index_and_composite_unique_annotations() {
        let src = r#"
entity Product "Product" {
  id: Int @pk
  sku: String @unique @index
  store_id: Int @index(status, store_id) @unique(sku, store_id)
  status: Enum(active, discontinued) @default(active)
}
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let product = model.entities.values().next().unwrap();
        let sku = product
            .columns
            .iter()
            .find(|column| column.name == "sku")
            .unwrap();
        assert!(sku.is_unique);
        assert!(sku.is_indexed);
        assert_eq!(
            product.unique_constraints,
            vec![
                vec!["sku".to_string()],
                vec!["sku".to_string(), "store_id".to_string()]
            ]
        );
        assert_eq!(
            product.indexes,
            vec![
                vec!["sku".to_string()],
                vec!["status".to_string(), "store_id".to_string()]
            ]
        );
    }

    #[test]
    fn test_build_model_data_modeling_annotations_and_fk_options() {
        let src = r#"
entity Customer "Customer" {
  id: Int @pk
}
entity Order "Order" {
  id: Int @pk
  tenant_id: Int @tenant
  total: Money @check("total >= 0")
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  net_total: Money @derived("total - discount")
}
relate(Order, Customer, "N:1").optional().on_delete(set_null).on_update(cascade)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let order = model
            .entities
            .iter()
            .find_map(|(_, entity)| (entity.id == "Order").then_some(entity))
            .unwrap();
        let tenant_id = order
            .columns
            .iter()
            .find(|column| column.name == "tenant_id")
            .unwrap();
        assert!(tenant_id.is_tenant_scope);
        let total = order
            .columns
            .iter()
            .find(|column| column.name == "total")
            .unwrap();
        assert_eq!(total.check_constraints, vec!["total >= 0"]);
        let deleted_at = order
            .columns
            .iter()
            .find(|column| column.name == "deleted_at")
            .unwrap();
        assert!(deleted_at.is_soft_delete);
        let valid_from = order
            .columns
            .iter()
            .find(|column| column.name == "valid_from")
            .unwrap();
        assert!(valid_from.is_history);
        let net_total = order
            .columns
            .iter()
            .find(|column| column.name == "net_total")
            .unwrap();
        assert_eq!(net_total.derived_expr.as_deref(), Some("total - discount"));
        let customer_id = order
            .columns
            .iter()
            .find(|column| column.name == "customer_id")
            .unwrap();
        assert!(customer_id.is_fk);
        assert!(customer_id.fk_optional);
        assert!(customer_id.is_nullable);
        assert_eq!(customer_id.fk_on_delete.as_deref(), Some("set_null"));
        assert_eq!(customer_id.fk_on_update.as_deref(), Some("cascade"));
    }

    #[test]
    fn test_build_model_rejects_requirement_metadata_on_other_kinds() {
        let src = r#"actor Customer "Customer" priority "must""#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (_, diags) = build_model(&ast);
        let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("requirement metadata is only valid")),
            "expected requirement metadata target diagnostic, got {messages:?}"
        );
    }

    #[test]
    fn test_build_model_rejects_api_metadata_on_other_kinds() {
        let src = r#"usecase PlaceOrder "Place order" method POST"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (_, diags) = build_model(&ast);
        let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("api metadata is only valid")),
            "expected api metadata target diagnostic, got {messages:?}"
        );
    }

    #[test]
    fn test_build_model_rejects_nfr_metadata_on_invalid_kinds() {
        let src = r#"quality Performance "Performance" metric p95_latency_ms"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (_, diags) = build_model(&ast);
        let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("non-functional metadata is only valid")),
            "expected nfr metadata target diagnostic, got {messages:?}"
        );
    }

    #[test]
    fn test_duplicate_definition_same_kind() {
        let src = r#"
actor Customer "顧客"
actor Customer "重複"
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("duplicate definition"));
    }

    #[test]
    fn test_same_name_different_kind_allowed() {
        // `actor Add` and `usecase Add` must coexist without error when
        // references are qualified.
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(actor::Add, usecase::Add)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        assert_eq!(model.actors.len(), 1);
        assert_eq!(model.use_cases.len(), 1);
        assert_eq!(model.relations.len(), 1);
    }

    #[test]
    fn test_ambiguous_unqualified_reference() {
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(Add, Add)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("ambiguous reference"));
    }

    #[test]
    fn test_type_mismatch() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Browse, Customer)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_nm_relation_warning() {
        let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
relate(A, B, "N:M")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
        assert!(!warnings.is_empty());
        assert!(warnings[0].error.to_string().contains("N:M relation"));
    }

    #[test]
    fn test_missing_pk_error() {
        let src = r#"
entity A "A" { name: String }
entity B "B" { id: Int @pk }
relate(B, A, "N:1")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("missing @pk"));
    }

    #[test]
    fn test_one_to_many_fk_on_to_side() {
        let src = r#"
entity Customer "顧客" { id: Int @pk }
entity Order "注文" { id: Int @pk }
relate(Customer, Order, "1:N")
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);

        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        let order = model.entities.values().find(|e| e.id == "Order").unwrap();
        let fk = order.columns.iter().find(|c| c.name == "customer_id");
        assert!(fk.is_some(), "customer_id FK not found in Order");
        assert!(fk.unwrap().is_fk);
    }

    #[test]
    fn test_api_declaration_and_invokes() {
        let src = r#"
usecase PlaceOrder "注文する"
api OrderApi "注文API" description "注文を永続化するAPI"
invokes(PlaceOrder, OrderApi)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.apis.len(), 1);
        let api = model.apis.values().next().unwrap();
        assert_eq!(api.id, "OrderApi");
        assert_eq!(api.label, "注文API");
        assert_eq!(api.description.as_deref(), Some("注文を永続化するAPI"));

        let invokes_rel = model.relations.iter().find(|r| r.kind == RelKind::Invokes);
        assert!(invokes_rel.is_some(), "Invokes relation should exist");
    }

    #[test]
    fn test_belongs_when_where_context() {
        let src = r#"
business ClinicOps "Clinic Operations"
buc BucAppointmentScheduling "Appointment Scheduling"
location FrontDesk "Front Desk"
timing AppointmentRequested "Appointment Requested"
medium FrontDeskTerminal "Front Desk Terminal"
belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .where(FrontDesk)
  .where("patient portal")
  .by(FrontDeskTerminal)
  .by("tablet")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let rel = model.relations.iter().find(|r| r.kind == RelKind::Belongs);
        assert!(rel.is_some(), "Belongs relation should still exist");

        assert_eq!(model.business_mapping_contexts.len(), 1);
        let ctx = &model.business_mapping_contexts[0];
        assert_eq!(model.bucs[ctx.buc].id, "BucAppointmentScheduling");
        assert_eq!(model.businesses[ctx.business].id, "ClinicOps");
        assert_eq!(ctx.whens.len(), 2);
        assert_eq!(ctx.wheres.len(), 2);
        assert_eq!(ctx.bys.len(), 2);

        assert!(matches!(
            &ctx.whens[0],
            BusinessMappingContextValue::Text(s) if s == "patient requests a booking"
        ));
        assert!(matches!(
            &ctx.whens[1],
            BusinessMappingContextValue::Ref(NodeRef::Timing(_))
        ));
        assert!(matches!(
            &ctx.wheres[0],
            BusinessMappingContextValue::Ref(NodeRef::Location(_))
        ));
        assert!(matches!(
            &ctx.wheres[1],
            BusinessMappingContextValue::Text(s) if s == "patient portal"
        ));
        assert!(matches!(
            &ctx.bys[0],
            BusinessMappingContextValue::Ref(NodeRef::Medium(_))
        ));
        assert!(matches!(
            &ctx.bys[1],
            BusinessMappingContextValue::Text(s) if s == "tablet"
        ));
    }

    #[test]
    fn test_actor_permission_attachment() {
        let src = r#"
actor Staff "Staff"
permission ManageSchedule "Manage Schedule"
has_permission(Staff, ManageSchedule)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.permissions.len(), 1);
        let permission = model.permissions.values().next().unwrap();
        assert_eq!(permission.id, "ManageSchedule");
        assert_eq!(permission.label, "Manage Schedule");

        let rel = model
            .relations
            .iter()
            .find(|r| r.kind == RelKind::HasPermission)
            .expect("HasPermission relation should exist");
        assert!(matches!(rel.from, NodeRef::Actor(_)));
        assert!(matches!(rel.to, NodeRef::Permission(_)));
    }

    #[test]
    fn after_assert_registers_temporal_assertion_from_equality_expr() {
        let (ast, parse_errors) = parse(
            r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" {
  id: Int @pk
  status: Enum(requested, executed) @default(requested)
}
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
        );
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.temporal_assertions.len(), 1);
        let assertion = &model.temporal_assertions[0];
        assert_eq!(model.use_cases[assertion.anchor].id, "ExecuteCertIssue");
        assert_eq!(assertion.requireds.len(), 1);
    }

    #[test]
    fn forbidden_when_none_registers_quantifier_constraint() {
        let (ast, parse_errors) = parse(
            r#"
entity ClientCertificate "Client Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity TerminalCertAssignment "Terminal Cert Assignment" {
  id: Int @pk
  status: Enum(active, inactive) @default(active)
}
forbidden_when(ClientCertificate, (status, revoked))
  .none(TerminalCertAssignment, (status, active))
"#,
        );
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.quantifier_constraints.len(), 1);
        let constraint = &model.quantifier_constraints[0];
        assert_eq!(model.entities[constraint.anchor].id, "ClientCertificate");
        assert_eq!(
            model.entities[constraint.related].id,
            "TerminalCertAssignment"
        );
        assert_eq!(constraint.guards.len(), 1);
        assert_eq!(constraint.related_conditions.len(), 1);
    }

    #[test]
    fn test_screen_constraint_patterns_derive_from_usecase_and_api() {
        let src = r#"
usecase BookAppointment "Book Appointment"
screen BookingScreen "Booking Screen"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
medium StaffTerminal "Staff Terminal"
medium SecureChannel "Secure Channel"
displays(BookAppointment, BookingScreen)
invokes(BookAppointment, BookingApi)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, PatientRead)
requires_medium(BookingApi, SecureChannel)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let patterns = crate::derive_screen_constraint_patterns(&model);
        assert_eq!(patterns.len(), 1);

        let pattern = &patterns[0];
        assert_eq!(model.screens[pattern.screen].id, "BookingScreen");
        assert_eq!(model.use_cases[pattern.usecase].id, "BookAppointment");
        assert_eq!(
            model.apis[pattern.api.expect("api should be part of the path")].id,
            "BookingApi"
        );

        let permission_ids: Vec<_> = pattern
            .permissions
            .iter()
            .map(|key| model.permissions[*key].id.as_str())
            .collect();
        assert_eq!(permission_ids, vec!["ScheduleWrite", "PatientRead"]);

        let medium_ids: Vec<_> = pattern
            .media
            .iter()
            .map(|key| model.media[*key].id.as_str())
            .collect();
        assert_eq!(medium_ids, vec!["StaffTerminal", "SecureChannel"]);
    }

    #[test]
    fn test_api_crud_type_check_ok() {
        let src = r#"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
creates(OrderApi, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let creates_rel = model.relations.iter().find(|r| r.kind == RelKind::Creates);
        assert!(creates_rel.is_some());
    }

    #[test]
    fn test_invokes_type_mismatch() {
        // invokes(uc, entity) は TypeMismatch になるはず
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
invokes(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty(), "type mismatch expected");
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_usecase_crud_still_allowed() {
        // 後方互換: usecase が直接 entity を creates しても OK
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
creates(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "legacy creates(uc, entity) should still work"
        );
        assert_eq!(
            model
                .relations
                .iter()
                .filter(|r| r.kind == RelKind::Creates)
                .count(),
            1
        );
    }

    #[test]
    fn test_sets_comparison_registers_proposition_effect() {
        let src = r#"
usecase Sell "販売する"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
}
updates(Sell, Stock)
sets(Sell, Stock, stock < selling, true)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.proposition_effects.len(), 1);
        let effect = &model.proposition_effects[0];
        assert_eq!(effect.prop.axis_key(), "stock<selling");
        assert!(effect.truth);
        assert!(matches!(effect.origin, NodeRef::UseCase(_)));
    }

    #[test]
    fn test_required_registers_conditions_and_comparison() {
        let src = r#"
entity Coupon "クーポン" {
  id: Int @pk
  status: Enum(usable, expired)
  expired_at: DateTime @null
}
required(Coupon, (status, usable), expired_at < now)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.required_constraints.len(), 1);
        let constraint = &model.required_constraints[0];
        assert_eq!(constraint.conditions.len(), 1);
        assert_eq!(constraint.comparisons.len(), 1);
        assert_eq!(constraint.comparisons[0].axis_key(), "expired_at<now");
    }

    #[test]
    fn test_exclusive_registers_flat_pair_conditions() {
        let src = r#"
entity Document "文書" {
  id: Int @pk
  approved: Bool
  rejected: Bool
}
exclusive(Document, approved, true, rejected, true)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.exclusive_constraints.len(), 1);
        let constraint = &model.exclusive_constraints[0];
        assert_eq!(constraint.conditions.len(), 2);
        assert_eq!(constraint.comparisons.len(), 0);
    }

    #[test]
    fn test_cross_forbidden_registers_qualified_conditions() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, cancelled)
  total: Decimal
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
  amount: Decimal
}
cross_forbidden(Order, Payment, (Order.status, cancelled), Payment.amount > Order.total)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.cross_forbidden_constraints.len(), 1);
        let constraint = &model.cross_forbidden_constraints[0];
        assert_eq!(constraint.scope.len(), 2);
        assert_eq!(constraint.conditions.len(), 2);

        let order_key = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Order").then_some(key))
            .unwrap();
        let payment_key = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Payment").then_some(key))
            .unwrap();

        assert!(matches!(
            &constraint.conditions[0],
            CrossEntityCondition::Equals { column, value }
                if column.entity == order_key
                    && column.column == "status"
                    && value == &EffectValue::EnumVariant("cancelled".to_string())
        ));
        assert!(matches!(
            &constraint.conditions[1],
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs,
                op: CmpOpModel::Gt,
                rhs: CrossCmpRhs::Column(rhs),
            }) if lhs.entity == payment_key
                && lhs.column == "amount"
                && rhs.entity == order_key
                && rhs.column == "total"
        ));
    }

    #[test]
    fn test_cross_invariant_registers_when_then_conditions() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.cross_entity_invariants.len(), 1);
        let invariant = &model.cross_entity_invariants[0];
        assert_eq!(invariant.scope.len(), 2);
        assert_eq!(invariant.guards.len(), 1);
        assert_eq!(invariant.requireds.len(), 1);
    }

    #[test]
    fn test_cross_invariant_registers_along_scope() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let invariant = &model.cross_entity_invariants[0];
        let CrossConstraintScope::RelationPath(path) = &invariant.scope_semantics else {
            panic!(
                "expected relation-path scope, got {:?}",
                invariant.scope_semantics
            );
        };
        let path_ids: Vec<_> = path
            .iter()
            .map(|key| model.entities[*key].id.as_str())
            .collect();
        assert_eq!(path_ids, vec!["Order", "Payment"]);
    }

    #[test]
    fn test_cross_invariant_can_infer_scope_from_qualified_columns() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant()
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let invariant = &model.cross_entity_invariants[0];
        let scope_ids: Vec<_> = invariant
            .scope
            .iter()
            .map(|key| model.entities[*key].id.as_str())
            .collect();
        assert_eq!(scope_ids, vec!["Order", "Payment"]);
    }

    #[test]
    fn test_cross_invariant_requires_column_qualifier_for_multi_entity_scope() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant(Order, Payment)
  .when(status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors
                .iter()
                .any(|d| d.error.to_string().contains("needs an entity qualifier")),
            "expected qualifier diagnostic, got: {:?}",
            errors
        );
    }
}
