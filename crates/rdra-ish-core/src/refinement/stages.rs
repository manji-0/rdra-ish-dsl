//! One `StageSpec` per stage: the text the docs and skills show, beside the checks of its
//! gate. The docs hold generated renderings of this text; editing it here is the only edit.

use std::collections::BTreeSet;

use super::{BucFacts, Check, Gap, Requirement, StageId, StageSpec, SubjectKind};
use crate::model::{ColumnType, EntityKey, SemanticModel};

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

/// A clause over every use case or entity: fails with no element to name when there is none.
fn all_of(total: usize, gaps: Vec<Gap>) -> Result<(), Vec<Gap>> {
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

fn ids(model: &SemanticModel, entities: &[EntityKey]) -> Vec<String> {
    entities
        .iter()
        .map(|&entity| model.entities[entity].id.clone())
        .collect()
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
            eval: |f| {
                if f.starters == 0 {
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
            passes_when: "it holds a use case (by `contains`, or through a flow's steps and `covers`)",
            eval: |f| {
                if f.use_cases.is_empty() {
                    let id = f.buc_id();
                    Err(vec![gap(
                        SubjectKind::Buc,
                        id,
                        format!(
                            "add a `usecase` and `contains({id}, UseCase)` for each user-visible action"
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
            f.starters
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
        "Are any entities only conceptual at this stage?",
        "Which entities should be shared across multiple BUCs?",
    ],
    review: &[
        "rdra-ish check {src}",
        "rdra-ish csv {src} --kind matrix",
        "rdra-ish diagram {src} --kind er --format mermaid --buc {buc}",
    ],
    requirement: Requirement::Required,
    checks: &[Check {
        passes_when: "every use case creates, reads, updates, or deletes an entity, directly or through an API it `invokes`",
        eval: |f| {
            let gaps = f
                .use_cases
                .iter()
                .filter(|&&uc| !f.touches(uc))
                .map(|&uc| {
                    let id = f.use_case_id(uc);
                    gap(
                        SubjectKind::UseCase,
                        id,
                        format!(
                            "add `creates` / `reads` / `updates` / `deletes({id}, Entity)` for the data it touches"
                        ),
                    )
                })
                .collect();
            all_of(f.use_cases.len(), gaps)
        },
    }],
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
    checks: &[Check {
        passes_when: "every use case `displays` a screen or `invokes` an API, or an event or timer `triggers` it",
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
            all_of(f.use_cases.len(), gaps)
        },
    }],
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
    checks: &[Check {
        passes_when:
            "every entity of stage 2 has a column that is neither a primary key nor a foreign key",
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
                        format!(
                            "add the columns that identify and describe {id}, and `relate` it to its parent"
                        ),
                    )
                })
                .collect();
            all_of(f.entities.len(), gaps)
        },
    }],
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
    checks: &[Check {
        passes_when: "a use case moves an entity with `transitions` or `sets`",
        eval: |f| {
            if !f.moved.is_empty() {
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
    }],
    evidence: |f| {
        if f.moved.is_empty() {
            "no use case moves an entity state".to_string()
        } else {
            format!("use cases move {}", ids(f.model, &f.moved).join(", "))
        }
    },
};

/// Which state rules a rules stage counts.
#[derive(Clone, Copy)]
enum RuleTier {
    /// `forbidden` / `exclusive` over plain facts.
    Guardrail,
    /// `invariant` / `required`.
    Obligation,
    /// A comparison proposition in any rule, or a rule over more than one entity.
    Advanced,
}

impl RuleTier {
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

    /// Entities some rule of the tier names.
    fn entities(self, model: &SemanticModel) -> Vec<EntityKey> {
        let mut entities = Vec::new();
        match self {
            RuleTier::Guardrail => {
                entities.extend(model.forbidden_constraints.iter().map(|c| c.entity));
                entities.extend(model.exclusive_constraints.iter().map(|c| c.entity));
            }
            RuleTier::Obligation => {
                entities.extend(model.entity_invariants.iter().map(|c| c.entity));
                entities.extend(model.required_constraints.iter().map(|c| c.entity));
            }
            RuleTier::Advanced => {
                entities.extend(
                    model
                        .forbidden_constraints
                        .iter()
                        .filter(|c| !c.comparisons.is_empty())
                        .map(|c| c.entity),
                );
                entities.extend(
                    model
                        .exclusive_constraints
                        .iter()
                        .filter(|c| !c.comparisons.is_empty())
                        .map(|c| c.entity),
                );
                entities.extend(
                    model
                        .entity_invariants
                        .iter()
                        .filter(|c| {
                            !c.guard_comparisons.is_empty() || !c.required_comparisons.is_empty()
                        })
                        .map(|c| c.entity),
                );
                entities.extend(
                    model
                        .required_constraints
                        .iter()
                        .filter(|c| !c.comparisons.is_empty())
                        .map(|c| c.entity),
                );
                for c in &model.cross_forbidden_constraints {
                    entities.extend(c.scope.iter().copied());
                }
                for c in &model.cross_entity_invariants {
                    entities.extend(c.scope.iter().copied());
                }
            }
        }
        entities
    }

    /// Ids of the entities this BUC moves that a rule of the tier names.
    fn ruled(self, f: &BucFacts) -> BTreeSet<String> {
        self.entities(f.model)
            .into_iter()
            .filter(|entity| f.moved.contains(entity))
            .map(|entity| f.entity_id(entity).to_string())
            .collect()
    }

    fn check(self, f: &BucFacts) -> Result<(), Vec<Gap>> {
        if !self.ruled(f).is_empty() {
            return Ok(());
        }
        Err(f
            .moved
            .iter()
            .map(|&entity| {
                let id = f.entity_id(entity);
                gap(SubjectKind::Entity, id, self.gap(id))
            })
            .collect())
    }

    fn evidence(self, f: &BucFacts) -> String {
        let ruled = self.ruled(f);
        if ruled.is_empty() {
            format!("no {} on an entity this BUC moves", self.kinds())
        } else {
            format!(
                "{} on {}",
                self.kinds(),
                ruled.into_iter().collect::<Vec<_>>().join(", ")
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
    requirement: Requirement::Required,
    checks: &[Check {
        passes_when: "a `forbidden` or `exclusive` names an entity of stage 5",
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
    requirement: Requirement::Required,
    checks: &[Check {
        passes_when: "an `invariant` or `required` names an entity of stage 5",
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
    requirement: Requirement::Required,
    checks: &[Check {
        passes_when: "a rule on an entity of stage 5 holds a comparison or names more than one entity",
        eval: |f| RuleTier::Advanced.check(f),
    }],
    evidence: |f| RuleTier::Advanced.evidence(f),
};
