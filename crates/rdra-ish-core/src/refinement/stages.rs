//! One `StageSpec` per stage: the text the docs and skills show, beside the checks of its gate.

use std::collections::BTreeSet;

use super::facts::Axis;
use super::{BucFacts, Check, Gap, Requirement, StageId, StageSpec, SubjectKind, Weight};
use crate::model::{
    CmpRhs, ColumnType, ComparisonProp, CrossCmpRhs, CrossEntityCondition, EffectValue, EntityKey,
    SemanticModel,
};

fn gap(subject_kind: SubjectKind, subject_id: &str, message: String) -> Gap {
    Gap {
        subject_kind,
        subject_id: subject_id.to_string(),
        message,
    }
}

fn failing(gaps: Vec<Gap>) -> Result<(), Vec<Gap>> {
    if gaps.is_empty() {
        Ok(())
    } else {
        Err(gaps)
    }
}

fn all_of_nonempty(total: usize, gaps: Vec<Gap>) -> Result<(), Vec<Gap>> {
    if total == 0 {
        Err(Vec::new())
    } else {
        failing(gaps)
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

pub(super) const SCOPE: StageSpec = StageSpec {
    id: StageId::Scope,
    name: "Scope sketch",
    concern: "Biz intent",
    question: "What business area are we modeling?",
    adds: "`business`, rough `buc`, `belongs`, candidate `actor`, optional `requirement` and `motivates`",
    asks: &[
        "Which business area or value stream is in scope?",
        "What are the candidate BUC names?",
        "Which BUC should be modeled first?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish list {src} --kind buc --format table",
        "rdra-ish diagram {src} --kind rdra --format mermaid --buc {buc}",
    ],
    requirement: Requirement::Required,
    checks: &[Check {
        passes_when: "the BUC `belongs` to a business",
        weight: Weight::Blocking,
        eval: |f| {
            if f.businesses.is_empty() {
                let id = f.buc_id();
                Err(vec![gap(
                    SubjectKind::Buc,
                    id,
                    format!("add `belongs({id}, Business)` to name its business area"),
                )])
            } else {
                Ok(())
            }
        },
    }],
    evidence: |f| {
        if f.businesses.is_empty() {
            "belongs to no business".to_string()
        } else {
            format!("belongs to {}", f.businesses.join(", "))
        }
    },
};

pub(super) const SKELETON: StageSpec = StageSpec {
    id: StageId::Skeleton,
    name: "BUC skeleton",
    concern: "Biz value",
    question: "Who gets value from each BUC?",
    adds: "`performs`, `contains`, rough `usecase`, `flow` / `step` when order matters",
    asks: &[
        "Who initiates or receives value from this BUC?",
        "What user-visible actions compose the BUC?",
        "Are any use cases triggered by a system event rather than a human actor?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish diagram {src} --kind rdra --format mermaid --buc {buc}",
    ],
    requirement: Requirement::Required,
    checks: &[
        Check {
            passes_when: "an actor `performs` it or one of its use cases, or an event or timer `triggers` one",
            weight: Weight::Blocking,
            eval: |f| {
                if f.performs_or_triggers == 0 {
                    let id = f.buc_id();
                    Err(vec![gap(
                        SubjectKind::Buc,
                        id,
                        format!(
                            "add `performs(Actor, {id})`, or `triggers(Event, {id})` when an event starts it"
                        ),
                    )])
                } else {
                    Ok(())
                }
            },
        },
        Check {
            passes_when: "it holds a use case (by `contains`, or through a flow's steps and `covers`), or an event or timer `triggers` the BUC itself",
            weight: Weight::Blocking,
            eval: |f| {
                if f.use_cases.is_empty() && f.buc_triggers.is_empty() {
                    let id = f.buc_id();
                    Err(vec![gap(
                        SubjectKind::Buc,
                        id,
                        format!(
                            "add a `usecase` and `contains({id}, UseCase)` for each user-visible action, or `triggers(Event, {id})` when an event starts it and its entry use case is not known yet"
                        ),
                    )])
                } else {
                    Ok(())
                }
            },
        },
    ],
    evidence: |f| {
        format!(
            "{}, {} `performs` / `triggers` starting them",
            count(f.use_cases.len(), "use case"),
            f.performs_or_triggers
        )
    },
};

pub(super) const DATA_TOUCHPOINTS: StageSpec = StageSpec {
    id: StageId::DataTouchpoints,
    name: "Data touchpoints",
    concern: "Biz object touchpoints",
    question: "Which data objects does each use case touch?",
    adds: "coarse `entity`, CRUD predicates (`creates`, `reads`, `updates`, `deletes`)",
    asks: &[
        "Which business objects are created, read, updated, or deleted by each use case?",
        "Which nouns are logical data structures, and which are only business vocabulary?",
        "Which entities should be shared across multiple BUCs?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish csv {src} --kind matrix",
        "rdra-ish diagram {src} --kind er --format mermaid --buc {buc}",
    ],
    requirement: Requirement::Required,
    checks: &[
        Check {
            passes_when: "the BUC holds a use case",
            weight: Weight::Blocking,
            eval: |f| {
                if !f.use_cases.is_empty() {
                    return Ok(());
                }
                let id = f.buc_id();
                let message = if f.buc_triggers.is_empty() {
                    format!("add a `usecase` and `contains({id}, UseCase)` for each user-visible action")
                } else {
                    format!(
                        "name the use case that {} starts: add a `usecase` and `contains({id}, UseCase)`",
                        f.buc_triggers.join(" / ")
                    )
                };
                Err(vec![gap(SubjectKind::Buc, id, message)])
            },
        },
        Check {
            passes_when: "a use case creates, reads, updates, or deletes an entity, directly or through an API it `invokes`",
            weight: Weight::Blocking,
            eval: |f| {
                if f.use_cases.is_empty() || f.use_cases.iter().any(|&uc| f.touches(uc)) {
                    return Ok(());
                }
                let id = f.buc_id();
                Err(vec![gap(
                    SubjectKind::Buc,
                    id,
                    "add `creates` / `reads` / `updates` / `deletes(UseCase, Entity)` for the data its use cases touch".to_string(),
                )])
            },
        },
        Check {
            passes_when: "review: once a use case touches data, each use case with no touchpoint is data-free on purpose",
            weight: Weight::Advisory,
            eval: |f| {
                if !f.use_cases.iter().any(|&uc| f.touches(uc)) {
                    return Ok(());
                }
                failing(
                    f.use_cases
                        .iter()
                        .filter(|&&uc| !f.touches(uc))
                        .map(|&uc| {
                            let id = f.use_case_id(uc);
                            gap(
                                SubjectKind::UseCase,
                                id,
                                format!(
                                    "is {id} data-free on purpose? otherwise add `creates` / `reads` / `updates` / `deletes({id}, Entity)`"
                                ),
                            )
                        })
                        .collect(),
                )
            },
        },
    ],
    evidence: |f| {
        let total = f.use_cases.len();
        let touching = f.use_cases.iter().filter(|&&uc| f.touches(uc)).count();
        format!("{touching}/{total} use cases touch an entity")
    },
};

pub(super) const INTERACTION_BOUNDARY: StageSpec = StageSpec {
    id: StageId::InteractionBoundary,
    name: "Interaction boundary",
    concern: "Tech interaction boundary",
    question: "What UI/API boundary mediates the work, and with which authority/media constraints?",
    adds: "`screen`, `displays`, `shows`, `field`, optional `api` / `invokes`, `dto`, `system`, `permission`, `medium`, `has_permission`, `requires_*`",
    asks: &[
        "Which screen or external interface does each use case expose?",
        "Does the use case write data directly, or through an API boundary?",
        "Is the API a reusable boundary or local to this BUC?",
        "Which actor permission and physical/device medium are required for the use case or API?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish diagram {src} --kind rdra --format mermaid --buc {buc}",
        "rdra-ish list {src} --kind api --format table",
        "rdra-ish csv {src} --kind api-matrix",
        "rdra-ish csv {src} --kind screen-constraints",
        "rdra-ish csv {src} --kind actor-permission-audit",
    ],
    requirement: Requirement::Required,
    checks: &[
        Check {
            passes_when: "every use case `displays` a screen or `invokes` an API, or an event or timer `triggers` it",
            weight: Weight::Blocking,
            eval: |f| {
                let gaps = f
                    .use_cases
                    .iter()
                    .filter(|&&uc| !f.has_boundary(uc))
                    .map(|&uc| {
                        let id = f.use_case_id(uc);
                        gap(
                            SubjectKind::UseCase,
                            id,
                            format!(
                                "add `displays({id}, Screen)` or `invokes({id}, Api)` for the boundary it works through"
                            ),
                        )
                    })
                    .collect();
                all_of_nonempty(f.use_cases.len(), gaps)
            },
        },
        Check {
            passes_when: "review: when the model declares a `permission` or `medium`, each use case with no `requires_permission` / `requires_medium` on it or its APIs is open to every actor and device on purpose",
            weight: Weight::Advisory,
            eval: |f| {
                if !f.has_access_vocabulary() {
                    return Ok(());
                }
                failing(
                    f.use_cases
                        .iter()
                        .filter(|&&uc| !f.requires_access(uc))
                        .map(|&uc| {
                            let id = f.use_case_id(uc);
                            gap(
                                SubjectKind::UseCase,
                                id,
                                format!(
                                    "{id} and its APIs require no permission or medium: open to every actor and device, or add `requires_permission` / `requires_medium`?"
                                ),
                            )
                        })
                        .collect(),
                )
            },
        },
    ],
    evidence: |f| {
        let total = f.use_cases.len();
        let bounded = f.use_cases.iter().filter(|&&uc| f.has_boundary(uc)).count();
        format!("{bounded}/{total} use cases display a screen, invoke an API, or run on a trigger")
    },
};

pub(super) const ENTITY_STRUCTURE: StageSpec = StageSpec {
    id: StageId::EntityStructure,
    name: "Entity structure",
    concern: "Tech data design",
    question: "What structure and ownership does the data need?",
    adds: "columns, `@pk`, `variation`, `relate`, cardinality, optional `owns` / `coordinates`",
    asks: &[
        "What fields identify each entity?",
        "Which fields are business state, and which are ordinary data?",
        "What parent/child relationships should own or group records?",
        "Does a use case need one transaction across related entities?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish diagram {src} --kind er --format mermaid --buc {buc}",
    ],
    requirement: Requirement::Required,
    checks: &[
        Check {
            passes_when: "every touched entity has a column that is neither a primary key nor a foreign key",
            weight: Weight::Blocking,
            eval: |f| {
                let gaps = f
                    .entities
                    .iter()
                    .filter(|&&entity| !f.has_attribute(entity))
                    .map(|&entity| {
                        let id = f.entity_id(entity);
                        gap(
                            SubjectKind::Entity,
                            id,
                            format!("add the columns that identify and describe {id}"),
                        )
                    })
                    .collect();
                all_of_nonempty(f.entities.len(), gaps)
            },
        },
        Check {
            passes_when: "review: when the BUC touches two or more entities, each touched entity with no `relate` to another touched entity stands alone on purpose",
            weight: Weight::Advisory,
            eval: |f| {
                if f.entities.len() < 2 {
                    return Ok(());
                }
                failing(
                    f.entities
                        .iter()
                        .filter(|&&entity| !f.related_to_touched(entity))
                        .map(|&entity| {
                            let id = f.entity_id(entity);
                            gap(
                                SubjectKind::Entity,
                                id,
                                format!(
                                    "{id} has no `relate` to another entity this BUC touches: standalone, or owned by one of them?"
                                ),
                            )
                        })
                        .collect(),
                )
            },
        },
    ],
    evidence: |f| {
        let total = f.entities.len();
        let described = f
            .entities
            .iter()
            .filter(|&&entity| f.has_attribute(entity))
            .count();
        format!("{described}/{total} entities have a column beyond their keys")
    },
};

pub(super) const LIFECYCLE: StageSpec = StageSpec {
    id: StageId::Lifecycle,
    name: "Lifecycle",
    concern: "Tech lifecycle design",
    question: "Which states and event-triggered BUC entries are reachable through the BUCs?",
    adds: "`Enum`, `Bool`, `@null`, `event`, `transitions`, `raises`, `triggers`, `timer`, `sets`, `condition`",
    asks: &[
        "Which Enum column and variants represent lifecycle state?",
        "Which use case causes each state change?",
        "Does any event start another BUC, and is the concrete entry use case known yet?",
        "Which nullable or boolean fields change together with the lifecycle?",
        "Are the Enum variants the states you want to see? Diagrams derive one state per variant.",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish states {src} --entity {entity}",
        "rdra-ish diagram {src} --kind state --format mermaid --buc {buc}",
        "rdra-ish diagram {src} --kind event --format mermaid",
    ],
    requirement: Requirement::Required,
    checks: &[
        Check {
            passes_when: "a use case moves an Enum or Bool column, with `transitions` or with `sets` on it",
            weight: Weight::Blocking,
            eval: |f| {
                if f.moved.iter().any(|m| m.axis == Axis::State) {
                    return Ok(());
                }
                let mut gaps = Vec::new();
                for &entity in &f.entities {
                    let decl = &f.model.entities[entity];
                    let axes: Vec<&str> = decl
                        .columns
                        .iter()
                        .filter(|col| matches!(col.col_type, ColumnType::Enum(_) | ColumnType::Bool))
                        .map(|col| col.name.as_str())
                        .collect();
                    if !axes.is_empty() {
                        gaps.push(gap(
                            SubjectKind::Entity,
                            &decl.id,
                            format!(
                                "add `transitions({}.<column>, UseCase, from -> to)` or `sets(UseCase, {}, ...)` for a use case that moves {}",
                                decl.id,
                                decl.id,
                                axes.join(" / ")
                            ),
                        ));
                    }
                }
                if gaps.is_empty() {
                    let id = f.buc_id();
                    gaps.push(gap(
                        SubjectKind::Buc,
                        id,
                        format!(
                            "add a lifecycle `Enum` column to the entity whose state {id} changes, and `transitions` for the use case that moves it"
                        ),
                    ));
                }
                Err(gaps)
            },
        },
        Check {
            passes_when: "review: a `sets` that only marks presence, an Int, or a comparison follows a lifecycle Enum or Bool",
            weight: Weight::Advisory,
            eval: |f| {
                if f.moved.iter().any(|m| m.axis == Axis::State) {
                    return Ok(());
                }
                failing(
                    f.moved
                        .iter()
                        .map(|m| {
                            let column = f.column_label(m.entity, &m.column);
                            gap(
                                SubjectKind::Entity,
                                f.entity_id(m.entity),
                                format!(
                                    "`sets` moves {column}, which is not an Enum or Bool: which lifecycle state does it follow?"
                                ),
                            )
                        })
                        .collect(),
                )
            },
        },
    ],
    evidence: |f| {
        let columns: Vec<String> = f
            .moved
            .iter()
            .map(|m| f.column_label(m.entity, &m.column))
            .collect();
        if columns.is_empty() {
            "no use case moves an entity column".to_string()
        } else if f.moved.iter().any(|m| m.axis == Axis::State) {
            format!("use cases move {}", columns.join(", "))
        } else {
            format!(
                "no use case moves an Enum or Bool column; `sets` moves {}",
                columns.join(", ")
            )
        }
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RuleTier {
    Guardrail,
    Obligation,
    Advanced,
}

pub(super) enum Rule<'m> {
    Forbidden(&'m crate::model::ForbiddenConstraint),
    Exclusive(&'m crate::model::ExclusiveConstraint),
    Invariant(&'m crate::model::EntityInvariant),
    Required(&'m crate::model::RequiredConstraint),
    CrossForbidden(&'m crate::model::CrossForbiddenConstraint),
    CrossInvariant(&'m crate::model::CrossEntityInvariant),
}

impl<'m> Rule<'m> {
    fn all(model: &'m SemanticModel) -> Vec<Rule<'m>> {
        let mut rules = Vec::new();
        rules.extend(model.forbidden_constraints.iter().map(Rule::Forbidden));
        rules.extend(model.exclusive_constraints.iter().map(Rule::Exclusive));
        rules.extend(model.entity_invariants.iter().map(Rule::Invariant));
        rules.extend(model.required_constraints.iter().map(Rule::Required));
        rules.extend(
            model
                .cross_forbidden_constraints
                .iter()
                .map(Rule::CrossForbidden),
        );
        rules.extend(
            model
                .cross_entity_invariants
                .iter()
                .map(Rule::CrossInvariant),
        );
        rules
    }

    pub(super) fn tier(&self) -> RuleTier {
        let has_comparison = match self {
            Rule::Forbidden(c) => !c.comparisons.is_empty(),
            Rule::Exclusive(c) => !c.comparisons.is_empty(),
            Rule::Invariant(c) => {
                !c.guard_comparisons.is_empty() || !c.required_comparisons.is_empty()
            }
            Rule::Required(c) => !c.comparisons.is_empty(),
            Rule::CrossForbidden(_) | Rule::CrossInvariant(_) => return RuleTier::Advanced,
        };
        match self {
            _ if has_comparison => RuleTier::Advanced,
            Rule::Forbidden(_) | Rule::Exclusive(_) => RuleTier::Guardrail,
            Rule::Invariant(_) | Rule::Required(_) => RuleTier::Obligation,
            Rule::CrossForbidden(_) | Rule::CrossInvariant(_) => RuleTier::Advanced,
        }
    }

    fn columns(&self) -> BTreeSet<(EntityKey, String)> {
        let mut columns = BTreeSet::new();
        match self {
            Rule::Forbidden(c) => {
                local_columns(&mut columns, c.entity, &c.conditions, &c.comparisons)
            }
            Rule::Exclusive(c) => {
                local_columns(&mut columns, c.entity, &c.conditions, &c.comparisons)
            }
            Rule::Required(c) => {
                local_columns(&mut columns, c.entity, &c.conditions, &c.comparisons)
            }
            Rule::Invariant(c) => {
                local_columns(&mut columns, c.entity, &c.guards, &c.guard_comparisons);
                local_columns(
                    &mut columns,
                    c.entity,
                    &c.requireds,
                    &c.required_comparisons,
                );
            }
            Rule::CrossForbidden(c) => cross_columns(&mut columns, &c.conditions),
            Rule::CrossInvariant(c) => {
                cross_columns(&mut columns, &c.guards);
                cross_columns(&mut columns, &c.requireds);
            }
        }
        columns
    }
}

fn local_columns(
    columns: &mut BTreeSet<(EntityKey, String)>,
    entity: EntityKey,
    conditions: &[(String, EffectValue)],
    comparisons: &[ComparisonProp],
) {
    for (column, _) in conditions {
        columns.insert((entity, column.clone()));
    }
    for cmp in comparisons {
        columns.insert((entity, cmp.lhs_column.clone()));
        if let CmpRhs::Column(rhs) = &cmp.rhs {
            columns.insert((entity, rhs.clone()));
        }
    }
}

fn cross_columns(columns: &mut BTreeSet<(EntityKey, String)>, conditions: &[CrossEntityCondition]) {
    for condition in conditions {
        match condition {
            CrossEntityCondition::Equals { column, .. } => {
                columns.insert((column.entity, column.column.clone()));
            }
            CrossEntityCondition::Comparison(cmp) => {
                columns.insert((cmp.lhs.entity, cmp.lhs.column.clone()));
                if let CrossCmpRhs::Column(rhs) = &cmp.rhs {
                    columns.insert((rhs.entity, rhs.column.clone()));
                }
            }
        }
    }
}

impl RuleTier {
    pub(super) fn stage(self) -> StageId {
        match self {
            RuleTier::Guardrail => StageId::LocalGuardrails,
            RuleTier::Obligation => StageId::LocalObligations,
            RuleTier::Advanced => StageId::AdvancedRules,
        }
    }

    fn kinds(self) -> &'static str {
        match self {
            RuleTier::Guardrail => "`forbidden` / `exclusive`",
            RuleTier::Obligation => "`invariant` / `required`",
            RuleTier::Advanced => "comparison or cross-entity rule",
        }
    }

    fn gap(self, id: &str) -> String {
        match self {
            RuleTier::Guardrail => format!(
                "add `forbidden({id}, ...)` or `exclusive({id}, ...)` for a combination of {id} facts that must never be reachable"
            ),
            RuleTier::Obligation => format!(
                "add `invariant({id}).when(...).then(...)` for facts that must co-occur, or a narrow `required({id}, ...)`"
            ),
            RuleTier::Advanced => format!(
                "add a comparison such as `forbidden({id}, a < b)` driven by `sets`, or a rule over {id} and another entity"
            ),
        }
    }

    fn credited(self, f: &BucFacts) -> BTreeSet<String> {
        let moved = f.moved_columns();
        Rule::all(f.model)
            .iter()
            .filter(|rule| rule.tier() == self)
            .flat_map(|rule| rule.columns())
            .filter(|column| moved.contains(column))
            .map(|(entity, column)| f.column_label(entity, &column))
            .collect()
    }

    fn check(self, f: &BucFacts) -> Result<(), Vec<Gap>> {
        if !self.credited(f).is_empty() {
            return Ok(());
        }
        Err(f
            .moved_entities()
            .into_iter()
            .map(|entity| {
                let id = f.entity_id(entity);
                gap(SubjectKind::Entity, id, self.gap(id))
            })
            .collect())
    }

    fn evidence(self, f: &BucFacts) -> String {
        let credited = self.credited(f);
        if credited.is_empty() {
            format!("no {} reads a column this BUC moves", self.kinds())
        } else {
            format!(
                "{} reads {}",
                self.kinds(),
                credited.into_iter().collect::<Vec<_>>().join(", ")
            )
        }
    }
}

const RULES_REVIEW: &[&str] = &[
    "rdra-ish check {src}",
    "rdra-ish states {src} --entity {entity}",
];

pub(super) const LOCAL_GUARDRAILS: StageSpec = StageSpec {
    id: StageId::LocalGuardrails,
    name: "Local guardrails",
    concern: "Tech-enforced rules",
    question: "Which simple states or facts must never co-occur?",
    adds: "`forbidden`, `exclusive` over Enum / Bool / nullable axes",
    asks: &[
        "Which combinations must never be reachable?",
        "Which facts must be mutually exclusive?",
        "Are these facts already represented as Enum, Bool, or nullable state axes?",
    ],
    review: RULES_REVIEW,
    requirement: Requirement::Optional,
    checks: &[Check {
        passes_when:
            "a `forbidden` or `exclusive` with no comparison reads a column this BUC moves",
        weight: Weight::Blocking,
        eval: |f| RuleTier::Guardrail.check(f),
    }],
    evidence: |f| RuleTier::Guardrail.evidence(f),
};

pub(super) const LOCAL_OBLIGATIONS: StageSpec = StageSpec {
    id: StageId::LocalObligations,
    name: "Local obligations",
    concern: "Tech-enforced rules",
    question: "Which reachable states imply or require other facts?",
    adds: "`invariant`, narrow `required`",
    asks: &[
        "Which values must co-occur once a condition is true?",
        "Which facts must be true in every reachable state?",
        "Is the rule too broad, or should it be an `invariant` with a guard instead of `required`?",
    ],
    review: RULES_REVIEW,
    requirement: Requirement::Optional,
    checks: &[Check {
        passes_when:
            "an `invariant` or `required` with no comparison reads a column this BUC moves",
        weight: Weight::Blocking,
        eval: |f| RuleTier::Obligation.check(f),
    }],
    evidence: |f| RuleTier::Obligation.evidence(f),
};

pub(super) const ADVANCED_RULES: StageSpec = StageSpec {
    id: StageId::AdvancedRules,
    name: "Advanced rules",
    concern: "Tech-enforced rules",
    question: "Which comparison or cross-entity facts are reviewable from the abstract state space?",
    adds: "comparison propositions, multi-entity `forbidden` / `invariant`",
    asks: &[
        "Which comparisons matter, and which use cases move the numeric or date values they read?",
        "Which rules really mention more than one entity?",
        "Is a cross-entity rule a global cross-product rule, or only intended along a declared relation path?",
    ],
    review: RULES_REVIEW,
    requirement: Requirement::Optional,
    checks: &[Check {
        passes_when: "a rule that holds a comparison or names more than one entity reads a column this BUC moves",
        weight: Weight::Blocking,
        eval: |f| RuleTier::Advanced.check(f),
    }],
    evidence: |f| RuleTier::Advanced.evidence(f),
};
