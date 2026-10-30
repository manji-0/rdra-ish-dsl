//! Where each BUC stands on the incremental modeling stage map, and what the next stage asks.
//!
//! The stages are the ones `docs/incremental-modeling.md` describes. Each stage has a gate a
//! BUC either passes or fails, and a BUC is at the highest stage whose gate it passes together
//! with every earlier gate. A failed gate names the elements that fail it, so the next stage
//! reads as a short list of gaps instead of a missing layer.

use std::collections::BTreeSet;

use crate::model::{BucKey, ColumnType, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

/// One stage of the incremental modeling flow, in refinement order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Scope,
    Skeleton,
    DataTouchpoints,
    InteractionBoundary,
    EntityStructure,
    Lifecycle,
    LocalGuardrails,
    LocalObligations,
    AdvancedRules,
}

impl Stage {
    pub const ALL: [Stage; 9] = [
        Stage::Scope,
        Stage::Skeleton,
        Stage::DataTouchpoints,
        Stage::InteractionBoundary,
        Stage::EntityStructure,
        Stage::Lifecycle,
        Stage::LocalGuardrails,
        Stage::LocalObligations,
        Stage::AdvancedRules,
    ];

    /// The stage number the Stage Map of `incremental-modeling.md` uses.
    pub fn number(self) -> &'static str {
        match self {
            Stage::Scope => "0",
            Stage::Skeleton => "1",
            Stage::DataTouchpoints => "2",
            Stage::InteractionBoundary => "3",
            Stage::EntityStructure => "4",
            Stage::Lifecycle => "5",
            Stage::LocalGuardrails => "6A",
            Stage::LocalObligations => "6B",
            Stage::AdvancedRules => "6C",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stage::Scope => "Scope sketch",
            Stage::Skeleton => "BUC skeleton",
            Stage::DataTouchpoints => "Data touchpoints",
            Stage::InteractionBoundary => "Interaction boundary",
            Stage::EntityStructure => "Entity structure",
            Stage::Lifecycle => "Lifecycle",
            Stage::LocalGuardrails => "Local guardrails",
            Stage::LocalObligations => "Local obligations",
            Stage::AdvancedRules => "Advanced rules",
        }
    }

    /// The anchor of the stage's section in `incremental-modeling.md`.
    pub fn doc_anchor(self) -> &'static str {
        match self {
            Stage::Scope => "stage-0-scope-sketch",
            Stage::Skeleton => "stage-1-buc-skeleton",
            Stage::DataTouchpoints => "stage-2-data-touchpoints",
            Stage::InteractionBoundary => "stage-3-interaction-boundary",
            Stage::EntityStructure => "stage-4-entity-structure",
            Stage::Lifecycle => "stage-5-lifecycle",
            Stage::LocalGuardrails => "stage-6a-local-guardrails",
            Stage::LocalObligations => "stage-6b-local-obligations",
            Stage::AdvancedRules => "stage-6c-comparison-and-cross-entity-rules",
        }
    }

    /// The main question of the stage, as the Stage Map words it.
    pub fn question(self) -> &'static str {
        match self {
            Stage::Scope => "What business area are we modeling?",
            Stage::Skeleton => "Who gets value from each BUC?",
            Stage::DataTouchpoints => "Which data objects does each use case touch?",
            Stage::InteractionBoundary => {
                "What UI/API boundary mediates the work, and with which authority/media constraints?"
            }
            Stage::EntityStructure => "What structure and ownership does the data need?",
            Stage::Lifecycle => {
                "Which states and event-triggered BUC entries are reachable through the BUCs?"
            }
            Stage::LocalGuardrails => "Which simple states or facts must never co-occur?",
            Stage::LocalObligations => "Which reachable states imply or require other facts?",
            Stage::AdvancedRules => {
                "Which comparison or cross-entity facts are reviewable from the abstract state space?"
            }
        }
    }

    /// The questions to ask the user before adding the stage, as the stage section lists them.
    pub fn asks(self) -> &'static [&'static str] {
        match self {
            Stage::Scope => &[
                "Which business area or value stream is in scope?",
                "What are the candidate BUC names?",
                "Which BUC should be modeled first?",
            ],
            Stage::Skeleton => &[
                "Who initiates or receives value from this BUC?",
                "What user-visible actions compose the BUC?",
                "Are any use cases triggered by a system event rather than a human actor?",
            ],
            Stage::DataTouchpoints => &[
                "Which business objects are created, read, updated, or deleted by each use case?",
                "Are any entities only conceptual at this stage?",
                "Which entities should be shared across multiple BUCs?",
            ],
            Stage::InteractionBoundary => &[
                "Which screen or external interface does each use case expose?",
                "Does the use case write data directly, or through an API boundary?",
                "Is the API a reusable boundary or local to this BUC?",
                "Which actor permission and physical/device medium are required for the use case or API?",
            ],
            Stage::EntityStructure => &[
                "What fields identify each entity?",
                "Which fields are business state, and which are ordinary data?",
                "What parent/child relationships should own or group records?",
                "Does a use case need one transaction across related entities?",
            ],
            Stage::Lifecycle => &[
                "Which Enum column and variants represent lifecycle state?",
                "Which use case causes each state change?",
                "Does any event start another BUC, and is the concrete entry use case known yet?",
                "Which nullable or boolean fields change together with the lifecycle?",
            ],
            Stage::LocalGuardrails => &[
                "Which combinations must never be reachable?",
                "Which facts must be mutually exclusive?",
                "Are these facts already represented as Enum, Bool, or nullable state axes?",
            ],
            Stage::LocalObligations => &[
                "Which values must co-occur once a condition is true?",
                "Which facts must be true in every reachable state?",
                "Is the rule too broad, or should it be an `invariant` with a guard instead of `required`?",
            ],
            Stage::AdvancedRules => &[
                "Which comparisons matter, and which use cases move the numeric or date values they read?",
                "Which rules really mention more than one entity?",
                "Is a cross-entity rule a global cross-product rule, or only intended along a declared relation path?",
            ],
        }
    }

    /// The commands that review the stage once it is added, as the stage section lists them.
    /// `{src}` stands for the model inputs and `{buc}` for the BUC id.
    pub fn validate(self) -> &'static [&'static str] {
        match self {
            Stage::Scope => &[
                "rdra-ish check {src}",
                "rdra-ish list {src} --kind buc --format table",
                "rdra-ish diagram {src} --kind rdra --buc {buc}",
            ],
            Stage::Skeleton => &[
                "rdra-ish check {src}",
                "rdra-ish diagram {src} --kind rdra --buc {buc}",
            ],
            Stage::DataTouchpoints => &[
                "rdra-ish check {src}",
                "rdra-ish csv {src} --kind matrix",
                "rdra-ish diagram {src} --kind er --buc {buc}",
            ],
            Stage::InteractionBoundary => &[
                "rdra-ish check {src}",
                "rdra-ish diagram {src} --kind rdra --buc {buc}",
                "rdra-ish csv {src} --kind api-matrix",
                "rdra-ish csv {src} --kind screen-constraints",
                "rdra-ish csv {src} --kind actor-permission-audit",
            ],
            Stage::EntityStructure => &[
                "rdra-ish check {src}",
                "rdra-ish diagram {src} --kind er --buc {buc}",
            ],
            Stage::Lifecycle => &[
                "rdra-ish check {src}",
                "rdra-ish states {src} --buc {buc}",
                "rdra-ish diagram {src} --kind state --buc {buc}",
                "rdra-ish diagram {src} --kind event",
            ],
            Stage::LocalGuardrails | Stage::LocalObligations | Stage::AdvancedRules => {
                &["rdra-ish states {src} --buc {buc}"]
            }
        }
    }

    pub fn next(self) -> Option<Stage> {
        Stage::ALL.get(self as usize + 1).copied()
    }
}

/// One element that keeps a BUC from passing a stage gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// The kind of the element the gap is about: `buc`, `usecase`, or `entity`.
    pub subject_kind: &'static str,
    pub subject_id: String,
    /// What is missing, worded as the predicate or declaration to add.
    pub message: String,
}

/// The result of one stage gate for one BUC.
#[derive(Debug, Clone)]
pub struct Gate {
    pub stage: Stage,
    pub passed: bool,
    /// What the gate counted, such as `2/3 use cases touch an entity`.
    pub evidence: String,
    pub gaps: Vec<Gap>,
}

/// Where one BUC stands on the stage map.
#[derive(Debug, Clone)]
pub struct BucProgress {
    pub buc_id: String,
    pub buc_label: String,
    /// One gate per stage, in stage order.
    pub gates: Vec<Gate>,
}

impl BucProgress {
    /// The highest stage whose gate passes together with every earlier gate. `None` when the
    /// scope gate already fails.
    pub fn stage(&self) -> Option<Stage> {
        self.gates
            .iter()
            .take_while(|gate| gate.passed)
            .last()
            .map(|gate| gate.stage)
    }

    /// The first failed gate: the stage to refine next. `None` once every gate passes.
    pub fn next_gate(&self) -> Option<&Gate> {
        self.gates.iter().find(|gate| !gate.passed)
    }
}

/// The stage of every BUC in `model`, in declaration order.
pub fn refinement_progress(model: &SemanticModel) -> Vec<BucProgress> {
    model
        .bucs
        .iter()
        .map(|(key, buc)| BucProgress {
            buc_id: buc.id.clone(),
            buc_label: buc.label.clone(),
            gates: BucFacts::collect(model, key).gates(),
        })
        .collect()
}

/// What a BUC holds, read once from the model relations.
struct BucFacts<'m> {
    model: &'m SemanticModel,
    buc: BucKey,
    use_cases: Vec<UseCaseKey>,
    /// Entities the BUC's use cases create, read, update, or delete, directly or through an
    /// invoked API, in first-touch order.
    entities: Vec<EntityKey>,
    /// Entities some use case of the BUC moves with `transitions` or `sets`.
    moved: Vec<EntityKey>,
}

impl<'m> BucFacts<'m> {
    fn collect(model: &'m SemanticModel, buc: BucKey) -> Self {
        let use_cases = buc_use_cases(model, buc);
        let mut entities = Vec::new();
        for &uc in &use_cases {
            for entity in use_case_entities(model, uc) {
                if !entities.contains(&entity) {
                    entities.push(entity);
                }
            }
        }
        let mut moved = Vec::new();
        let moves = model
            .state_transitions
            .iter()
            .map(|t| (t.usecase, t.entity))
            .chain(model.column_effects.iter().map(|e| (e.origin, e.entity)))
            .chain(
                model
                    .proposition_effects
                    .iter()
                    .map(|e| (e.origin, e.entity)),
            );
        for (uc, entity) in moves {
            if use_cases.contains(&uc) && !moved.contains(&entity) {
                moved.push(entity);
            }
        }
        Self {
            model,
            buc,
            use_cases,
            entities,
            moved,
        }
    }

    fn buc_id(&self) -> String {
        self.model.bucs[self.buc].id.clone()
    }

    fn gates(&self) -> Vec<Gate> {
        Stage::ALL
            .iter()
            .map(|&stage| match stage {
                Stage::Scope => self.scope(),
                Stage::Skeleton => self.skeleton(),
                Stage::DataTouchpoints => self.data_touchpoints(),
                Stage::InteractionBoundary => self.interaction_boundary(),
                Stage::EntityStructure => self.entity_structure(),
                Stage::Lifecycle => self.lifecycle(),
                Stage::LocalGuardrails => self.rules(stage, RuleTier::Guardrail),
                Stage::LocalObligations => self.rules(stage, RuleTier::Obligation),
                Stage::AdvancedRules => self.rules(stage, RuleTier::Advanced),
            })
            .collect()
    }

    fn scope(&self) -> Gate {
        let businesses: Vec<String> = self
            .model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Belongs && rel.from == NodeRef::Buc(self.buc))
            .filter_map(|rel| match rel.to {
                NodeRef::Business(key) => Some(self.model.businesses[key].id.clone()),
                _ => None,
            })
            .collect();
        let id = self.buc_id();
        if businesses.is_empty() {
            Gate {
                stage: Stage::Scope,
                passed: false,
                evidence: "belongs to no business".to_string(),
                gaps: vec![Gap {
                    subject_kind: "buc",
                    subject_id: id.clone(),
                    message: format!("add `belongs({id}, Business)` to name its business area"),
                }],
            }
        } else {
            Gate {
                stage: Stage::Scope,
                passed: true,
                evidence: format!("belongs to {}", businesses.join(", ")),
                gaps: Vec::new(),
            }
        }
    }

    fn skeleton(&self) -> Gate {
        let id = self.buc_id();
        let mut starters = 0;
        for rel in &self.model.relations {
            let starts = match rel.kind {
                RelKind::Performs => self.enters(&rel.to),
                RelKind::Triggers => {
                    matches!(rel.from, NodeRef::Event(_) | NodeRef::Timer(_))
                        && self.enters(&rel.to)
                }
                _ => false,
            };
            if starts {
                starters += 1;
            }
        }
        let mut gaps = Vec::new();
        if starters == 0 {
            gaps.push(Gap {
                subject_kind: "buc",
                subject_id: id.clone(),
                message: format!(
                    "add `performs(Actor, {id})`, or `triggers(Event, {id})` when an event starts it"
                ),
            });
        }
        if self.use_cases.is_empty() {
            gaps.push(Gap {
                subject_kind: "buc",
                subject_id: id.clone(),
                message: format!(
                    "add a `usecase` and `contains({id}, UseCase)` for each user-visible action"
                ),
            });
        }
        Gate {
            stage: Stage::Skeleton,
            passed: gaps.is_empty(),
            evidence: format!(
                "{}, {starters} `performs` / `triggers` starting them",
                count(self.use_cases.len(), "use case"),
            ),
            gaps,
        }
    }

    /// The relation target is this BUC or one of its use cases.
    fn enters(&self, to: &NodeRef) -> bool {
        match to {
            NodeRef::Buc(key) => *key == self.buc,
            NodeRef::UseCase(key) => self.use_cases.contains(key),
            _ => false,
        }
    }

    fn data_touchpoints(&self) -> Gate {
        let gaps: Vec<Gap> = self
            .use_cases
            .iter()
            .filter(|&&uc| use_case_entities(self.model, uc).is_empty())
            .map(|&uc| {
                let id = &self.model.use_cases[uc].id;
                Gap {
                    subject_kind: "usecase",
                    subject_id: id.clone(),
                    message: format!(
                        "add `creates` / `reads` / `updates` / `deletes({id}, Entity)` for the data it touches"
                    ),
                }
            })
            .collect();
        self.per_use_case(Stage::DataTouchpoints, "touch an entity", gaps)
    }

    fn interaction_boundary(&self) -> Gate {
        let gaps: Vec<Gap> = self
            .use_cases
            .iter()
            .filter(|&&uc| !self.has_boundary(uc))
            .map(|&uc| {
                let id = &self.model.use_cases[uc].id;
                Gap {
                    subject_kind: "usecase",
                    subject_id: id.clone(),
                    message: format!(
                        "add `displays({id}, Screen)` or `invokes({id}, Api)` for the boundary it works through"
                    ),
                }
            })
            .collect();
        self.per_use_case(
            Stage::InteractionBoundary,
            "display a screen, invoke an API, or run on a trigger",
            gaps,
        )
    }

    /// The use case works through a screen or an API, or an event or timer runs it.
    fn has_boundary(&self, uc: UseCaseKey) -> bool {
        let node = NodeRef::UseCase(uc);
        self.model.relations.iter().any(|rel| match rel.kind {
            RelKind::Displays | RelKind::Invokes => rel.from == node,
            RelKind::Triggers => rel.to == node,
            _ => false,
        })
    }

    fn per_use_case(&self, stage: Stage, what: &str, gaps: Vec<Gap>) -> Gate {
        let total = self.use_cases.len();
        Gate {
            stage,
            passed: total > 0 && gaps.is_empty(),
            evidence: format!("{}/{} use cases {what}", total - gaps.len(), total),
            gaps,
        }
    }

    fn entity_structure(&self) -> Gate {
        let gaps: Vec<Gap> = self
            .entities
            .iter()
            .filter(|&&entity| !self.has_attribute(entity))
            .map(|&entity| {
                let id = &self.model.entities[entity].id;
                Gap {
                    subject_kind: "entity",
                    subject_id: id.clone(),
                    message: format!(
                        "add the columns that identify and describe {id}, and `relate` it to its parent"
                    ),
                }
            })
            .collect();
        let total = self.entities.len();
        Gate {
            stage: Stage::EntityStructure,
            passed: total > 0 && gaps.is_empty(),
            evidence: format!(
                "{}/{} entities have a column beyond their keys",
                total - gaps.len(),
                total
            ),
            gaps,
        }
    }

    /// The entity has an authored column that is neither its primary key nor a foreign key.
    fn has_attribute(&self, entity: EntityKey) -> bool {
        self.model.entities[entity]
            .columns
            .iter()
            .any(|col| !col.is_pk && self.model.column_fk(entity, &col.name).is_none())
    }

    fn lifecycle(&self) -> Gate {
        if !self.moved.is_empty() {
            return Gate {
                stage: Stage::Lifecycle,
                passed: true,
                evidence: format!("use cases move {}", ids(self.model, &self.moved).join(", ")),
                gaps: Vec::new(),
            };
        }
        let mut gaps = Vec::new();
        for &entity in &self.entities {
            let decl = &self.model.entities[entity];
            let axes: Vec<&str> = decl
                .columns
                .iter()
                .filter(|col| matches!(col.col_type, ColumnType::Enum(_) | ColumnType::Bool))
                .map(|col| col.name.as_str())
                .collect();
            if !axes.is_empty() {
                gaps.push(Gap {
                    subject_kind: "entity",
                    subject_id: decl.id.clone(),
                    message: format!(
                        "add `transitions({}.<column>, UseCase, from -> to)` or `sets(UseCase, {}, ...)` for a use case that moves {}",
                        decl.id,
                        decl.id,
                        axes.join(" / ")
                    ),
                });
            }
        }
        if gaps.is_empty() {
            let id = self.buc_id();
            gaps.push(Gap {
                subject_kind: "buc",
                subject_id: id.clone(),
                message: format!(
                    "add a lifecycle `Enum` column to the entity whose state {id} changes, and `transitions` for the use case that moves it"
                ),
            });
        }
        Gate {
            stage: Stage::Lifecycle,
            passed: false,
            evidence: "no use case moves an entity state".to_string(),
            gaps,
        }
    }

    fn rules(&self, stage: Stage, tier: RuleTier) -> Gate {
        let ruled: BTreeSet<String> = rule_entities(self.model, tier)
            .into_iter()
            .filter(|entity| self.moved.contains(entity))
            .map(|entity| self.model.entities[entity].id.clone())
            .collect();
        if !ruled.is_empty() {
            return Gate {
                stage,
                passed: true,
                evidence: format!(
                    "{} on {}",
                    tier.kinds(),
                    ruled.into_iter().collect::<Vec<_>>().join(", ")
                ),
                gaps: Vec::new(),
            };
        }
        let gaps = self
            .moved
            .iter()
            .map(|&entity| {
                let id = &self.model.entities[entity].id;
                Gap {
                    subject_kind: "entity",
                    subject_id: id.clone(),
                    message: tier.gap(id),
                }
            })
            .collect();
        Gate {
            stage,
            passed: false,
            evidence: format!("no {} on an entity this BUC moves", tier.kinds()),
            gaps,
        }
    }
}

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
}

/// Entities some rule of `tier` names.
fn rule_entities(model: &SemanticModel, tier: RuleTier) -> Vec<EntityKey> {
    let mut entities = Vec::new();
    match tier {
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

/// The use cases a BUC holds: those it `contains`, and those its flows hold through
/// `contains` and `covers`, in first-reached order.
fn buc_use_cases(model: &SemanticModel, buc: BucKey) -> Vec<UseCaseKey> {
    let mut use_cases = Vec::new();
    let mut visited = vec![NodeRef::Buc(buc)];
    let mut queue = vec![NodeRef::Buc(buc)];
    while let Some(current) = queue.pop() {
        for rel in &model.relations {
            if !matches!(rel.kind, RelKind::Contains | RelKind::Covers) || rel.from != current {
                continue;
            }
            match &rel.to {
                NodeRef::UseCase(uc) => {
                    if !use_cases.contains(uc) {
                        use_cases.push(*uc);
                    }
                }
                to @ (NodeRef::Flow(_) | NodeRef::Step(_)) if !visited.contains(to) => {
                    visited.push(to.clone());
                    queue.push(to.clone());
                }
                _ => {}
            }
        }
    }
    use_cases
}

/// Entities a use case creates, reads, updates, or deletes, directly or through an invoked API.
fn use_case_entities(model: &SemanticModel, uc: UseCaseKey) -> Vec<EntityKey> {
    let node = NodeRef::UseCase(uc);
    let apis: Vec<NodeRef> = model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Invokes && rel.from == node)
        .map(|rel| rel.to.clone())
        .collect();
    let mut entities = Vec::new();
    for rel in &model.relations {
        let crud = matches!(
            rel.kind,
            RelKind::Creates
                | RelKind::Reads
                | RelKind::Updates
                | RelKind::Deletes
                | RelKind::Writes
        );
        if !crud || !(rel.from == node || apis.contains(&rel.from)) {
            continue;
        }
        if let NodeRef::Entity(entity) = rel.to {
            if !entities.contains(&entity) {
                entities.push(entity);
            }
        }
    }
    entities
}

fn ids(model: &SemanticModel, entities: &[EntityKey]) -> Vec<String> {
    entities
        .iter()
        .map(|&entity| model.entities[entity].id.clone())
        .collect()
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn progress(src: &str) -> Vec<BucProgress> {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        refinement_progress(&model)
    }

    fn gate(p: &BucProgress, stage: Stage) -> &Gate {
        p.gates.iter().find(|g| g.stage == stage).unwrap()
    }

    const SKELETON: &str = r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
actor Customer "Customer"
usecase PlaceOrder "Place Order"
belongs(BucOrder, Commerce)
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
"#;

    #[test]
    fn a_buc_without_business_is_before_stage_zero() {
        let p = &progress(r#"buc BucOrder "Process Order""#)[0];
        assert_eq!(p.stage(), None);
        let next = p.next_gate().unwrap();
        assert_eq!(next.stage, Stage::Scope);
        assert_eq!(next.gaps[0].subject_id, "BucOrder");
    }

    #[test]
    fn the_stage_is_the_last_gate_of_the_passing_prefix() {
        // Entity structure is met, but the use case has no boundary yet.
        let src = format!(
            "{SKELETON}
entity Order \"Order\" {{
  id: Int @pk
  total: Money
}}
creates(PlaceOrder, Order)
"
        );
        let p = &progress(&src)[0];
        assert!(gate(p, Stage::EntityStructure).passed);
        assert_eq!(p.stage(), Some(Stage::DataTouchpoints));
        let next = p.next_gate().unwrap();
        assert_eq!(next.stage, Stage::InteractionBoundary);
        assert_eq!(next.gaps.len(), 1);
        assert_eq!(next.gaps[0].subject_id, "PlaceOrder");
    }

    #[test]
    fn flow_steps_bring_their_use_cases_into_the_buc() {
        let src = r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
actor Customer "Customer"
flow OrderFlow "Order flow"
step Checkout "Checkout"
usecase PlaceOrder "Place Order"
belongs(BucOrder, Commerce)
performs(Customer, BucOrder)
contains(BucOrder, OrderFlow)
contains(OrderFlow, Checkout)
covers(Checkout, PlaceOrder)
"#;
        let p = &progress(src)[0];
        assert_eq!(p.stage(), Some(Stage::Skeleton));
        assert_eq!(
            gate(p, Stage::DataTouchpoints).gaps[0].subject_id,
            "PlaceOrder"
        );
    }

    #[test]
    fn api_crud_counts_as_a_touchpoint_and_a_triggered_use_case_needs_no_screen() {
        let src = r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
timer Nightly "Nightly"
usecase CloseOrders "Close Orders"
api OrderApi "Order API"
entity Order "Order" {
  id: Int @pk
}
belongs(BucOrder, Commerce)
contains(BucOrder, CloseOrders)
triggers(Nightly, CloseOrders)
invokes(CloseOrders, OrderApi)
updates(OrderApi, Order)
"#;
        let p = &progress(src)[0];
        assert!(gate(p, Stage::Skeleton).passed);
        assert!(gate(p, Stage::DataTouchpoints).passed);
        assert!(gate(p, Stage::InteractionBoundary).passed);
        assert_eq!(p.stage(), Some(Stage::InteractionBoundary));
    }

    #[test]
    fn a_foreign_key_is_not_an_attribute() {
        let src = format!(
            "{SKELETON}
screen Checkout \"Checkout\"
entity Customer2 \"Customer record\" {{
  id: Int @pk
  name: String
}}
entity Order \"Order\" {{
  id: Int @pk
}}
displays(PlaceOrder, Checkout)
creates(PlaceOrder, Order)
relate(Order, Customer2, N:1)
"
        );
        let p = &progress(&src)[0];
        let structure = gate(p, Stage::EntityStructure);
        assert!(!structure.passed);
        assert_eq!(structure.gaps[0].subject_id, "Order");
    }

    #[test]
    fn rules_count_only_on_entities_the_buc_moves() {
        let src = format!(
            "{SKELETON}
screen Checkout \"Checkout\"
entity Order \"Order\" {{
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  paid_at: DateTime @null
}}
entity Other \"Other\" {{
  id: Int @pk
  open: Bool @default(true)
}}
displays(PlaceOrder, Checkout)
creates(PlaceOrder, Order)
transitions(Order.status, PlaceOrder, pending -> paid)
sets(PlaceOrder, Order, paid_at == present)
forbidden(Other, open == false)
"
        );
        let p = &progress(&src)[0];
        assert_eq!(p.stage(), Some(Stage::Lifecycle));
        let guardrails = p.next_gate().unwrap();
        assert_eq!(guardrails.stage, Stage::LocalGuardrails);
        assert_eq!(guardrails.gaps[0].subject_id, "Order");
    }

    #[test]
    fn lifecycle_gaps_name_enum_and_bool_columns() {
        let src = format!(
            "{SKELETON}
screen Checkout \"Checkout\"
entity Order \"Order\" {{
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  note: String @null
}}
displays(PlaceOrder, Checkout)
creates(PlaceOrder, Order)
"
        );
        let p = &progress(&src)[0];
        let lifecycle = p.next_gate().unwrap();
        assert_eq!(lifecycle.stage, Stage::Lifecycle);
        assert!(lifecycle.gaps[0].message.contains("status"));
        assert!(!lifecycle.gaps[0].message.contains("note"));
    }
}
