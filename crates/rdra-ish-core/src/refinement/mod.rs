//! Where each BUC stands on the incremental modeling stage map, and what the next stage asks.
//!
//! The stages are the ones `docs/incremental-modeling.md` describes. `stages` holds one
//! `StageSpec` per stage: its text and the checks of its gate, side by side. `order::ORDER`
//! is the only place the stage order lives; numbers and doc anchors derive from it. A verdict
//! is derived from the checks and never stored, so nothing can disagree with the gaps.

#[cfg(test)]
mod docs;
mod facts;
mod order;
mod stages;

use crate::model::SemanticModel;

pub(crate) use facts::BucFacts;

/// One stage of the incremental modeling flow. Identity only: the order is `order::ORDER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageId {
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

impl StageId {
    /// Stable machine id for JSON and skills, never a number, so a reorder does not break
    /// consumers.
    pub fn slug(self) -> &'static str {
        match self {
            StageId::Scope => "scope",
            StageId::Skeleton => "skeleton",
            StageId::DataTouchpoints => "data-touchpoints",
            StageId::InteractionBoundary => "interaction-boundary",
            StageId::EntityStructure => "entity-structure",
            StageId::Lifecycle => "lifecycle",
            StageId::LocalGuardrails => "local-guardrails",
            StageId::LocalObligations => "local-obligations",
            StageId::AdvancedRules => "advanced-rules",
        }
    }

    pub fn spec(self) -> &'static StageSpec {
        spec(self)
    }
}

/// Everything code, docs, and skills say about one stage.
pub struct StageSpec {
    pub id: StageId,
    /// `Data touchpoints`: the Stage Map's Abstraction column and the doc heading
    /// `Stage {number}: {name}`.
    pub name: &'static str,
    /// The Stage Map's Concern column. Consecutive stages with one concern share a number
    /// and are told apart by a letter (`6A`, `6B`, `6C`).
    pub concern: &'static str,
    /// The main question of the stage, as the Stage Map words it.
    pub question: &'static str,
    /// The Stage Map's Add column: the DSL the stage adds.
    pub adds: &'static str,
    /// The questions to ask the user before adding the stage.
    pub asks: &'static [&'static str],
    /// The commands that review the stage once it is added. `{src}` stands for the model
    /// inputs, `{buc}` for the BUC id, and `{entity}` for one entity the BUC moves: a
    /// template with `{entity}` is repeated per entity.
    pub review: &'static [&'static str],
    pub requirement: Requirement,
    pub(crate) checks: &'static [Check],
    /// What the gate counted, such as `2/3 use cases touch an entity`.
    pub(crate) evidence: fn(&BucFacts) -> String,
}

impl StageSpec {
    /// The clauses of the gate, as the gate table of `docs/cli-reference.md#next` prints
    /// them.
    pub fn passes_when(&self) -> Vec<&'static str> {
        self.checks.iter().map(|check| check.passes_when).collect()
    }

    /// `review` with the placeholders filled: one command per template, and one per entity
    /// for a template with `{entity}`.
    pub fn review_commands(&self, src: &str, buc: &str, entities: &[&str]) -> Vec<String> {
        self.review
            .iter()
            .flat_map(|template| {
                let filled = template.replace("{src}", src).replace("{buc}", buc);
                if template.contains("{entity}") {
                    entities
                        .iter()
                        .map(|entity| filled.replace("{entity}", entity))
                        .collect()
                } else {
                    vec![filled]
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// Part of the prefix that decides `BucStanding::reached`.
    Required,
    /// Offered once the required stages are met; never holds a BUC back.
    Optional,
}

/// One clause of a gate. A check reads only `BucFacts`, never another stage's verdict, so
/// a gate means the same thing in any order.
pub(crate) struct Check {
    /// The clause as the gate table of `docs/cli-reference.md#next` prints it.
    pub passes_when: &'static str,
    /// `Ok` when the clause passes; `Err` names the elements that fail it. A clause over an
    /// empty set (no use case yet, no touched entity yet) fails with no element to name, so
    /// a stage is not ticked on the strength of nothing to check.
    pub eval: fn(&BucFacts) -> Result<(), Vec<Gap>>,
}

/// The stage's text and checks. An exhaustive match: a new stage does not compile until it
/// has a spec.
pub fn spec(id: StageId) -> &'static StageSpec {
    match id {
        StageId::Scope => &stages::SCOPE,
        StageId::Skeleton => &stages::SKELETON,
        StageId::DataTouchpoints => &stages::DATA_TOUCHPOINTS,
        StageId::InteractionBoundary => &stages::INTERACTION_BOUNDARY,
        StageId::EntityStructure => &stages::ENTITY_STRUCTURE,
        StageId::Lifecycle => &stages::LIFECYCLE,
        StageId::LocalGuardrails => &stages::LOCAL_GUARDRAILS,
        StageId::LocalObligations => &stages::LOCAL_OBLIGATIONS,
        StageId::AdvancedRules => &stages::ADVANCED_RULES,
    }
}

/// The stages in refinement order.
pub fn stages() -> impl Iterator<Item = StageId> {
    order::ORDER.iter().copied()
}

/// One element that fails a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub subject_kind: SubjectKind,
    pub subject_id: String,
    /// What is missing, worded as the predicate or declaration to add.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    Buc,
    UseCase,
    Entity,
}

impl SubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectKind::Buc => "buc",
            SubjectKind::UseCase => "usecase",
            SubjectKind::Entity => "entity",
        }
    }
}

/// One stage for one BUC: what each check found.
#[derive(Debug, Clone)]
pub struct StageView {
    pub id: StageId,
    pub evidence: String,
    /// Elements that fail a check.
    pub gaps: Vec<Gap>,
    open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Every check passes.
    Met,
    /// A check fails: `gaps` says what to add.
    Open,
}

impl StageView {
    fn evaluate(id: StageId, facts: &BucFacts) -> Self {
        let spec = spec(id);
        let mut gaps = Vec::new();
        let mut open = false;
        for check in spec.checks {
            if let Err(failing) = (check.eval)(facts) {
                open = true;
                gaps.extend(failing);
            }
        }
        StageView {
            id,
            evidence: (spec.evidence)(facts),
            gaps,
            open,
        }
    }

    /// Derived, never stored.
    pub fn verdict(&self) -> Verdict {
        if self.open {
            Verdict::Open
        } else {
            Verdict::Met
        }
    }

    pub fn spec(&self) -> &'static StageSpec {
        spec(self.id)
    }
}

/// Where one BUC stands: one view per stage, in order. There is no stored current stage;
/// `reached` and `next` derive from the views.
#[derive(Debug, Clone)]
pub struct BucStanding {
    pub id: String,
    pub label: String,
    /// Ids of the entities some use case of the BUC moves, for the `{entity}` review commands.
    pub moved: Vec<String>,
    pub stages: Vec<StageView>,
}

/// What to work on next.
pub enum Next<'a> {
    /// A required stage is not met: ask its questions and close its gaps.
    Required(&'a StageView),
    /// Every required stage is met. The optional stages are offered, never demanded.
    Optional(Vec<&'a StageView>),
}

impl BucStanding {
    fn required(&self) -> impl Iterator<Item = &StageView> {
        self.stages
            .iter()
            .filter(|view| view.spec().requirement == Requirement::Required)
    }

    /// The last stage of the met prefix of required stages. `None` when the first required
    /// stage is not met ("before stage 0").
    pub fn reached(&self) -> Option<StageId> {
        self.required()
            .take_while(|view| view.verdict() == Verdict::Met)
            .last()
            .map(|view| view.id)
    }

    pub fn next(&self) -> Next<'_> {
        match self.required().find(|view| view.verdict() != Verdict::Met) {
            Some(view) => Next::Required(view),
            None => Next::Optional(
                self.stages
                    .iter()
                    .filter(|view| view.spec().requirement == Requirement::Optional)
                    .collect(),
            ),
        }
    }

    pub fn view(&self, id: StageId) -> &StageView {
        self.stages
            .iter()
            .find(|view| view.id == id)
            .expect("every stage has a view")
    }
}

/// Where every BUC of a model stands, in declaration order.
pub struct Assessment {
    pub bucs: Vec<BucStanding>,
}

/// A pure function of the model: running it twice gives the same value.
pub fn assess(model: &SemanticModel) -> Assessment {
    Assessment {
        bucs: model
            .bucs
            .iter()
            .map(|(key, buc)| {
                let facts = BucFacts::collect(model, key);
                BucStanding {
                    id: buc.id.clone(),
                    label: buc.label.clone(),
                    moved: facts
                        .moved
                        .iter()
                        .map(|&entity| facts.entity_id(entity).to_string())
                        .collect(),
                    stages: stages().map(|id| StageView::evaluate(id, &facts)).collect(),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn standing(src: &str) -> Vec<BucStanding> {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assess(&model).bucs
    }

    fn next_required(p: &BucStanding) -> &StageView {
        match p.next() {
            Next::Required(view) => view,
            Next::Optional(_) => panic!("every required stage is met"),
        }
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
    fn numbers_and_anchors_derive_from_the_order() {
        let numbers: Vec<String> = stages().map(|id| id.number()).collect();
        assert_eq!(numbers, ["0", "1", "2", "3", "4", "5", "6A", "6B", "6C"]);
        assert_eq!(StageId::Scope.doc_anchor(), "stage-0-scope-sketch");
        assert_eq!(
            StageId::AdvancedRules.doc_anchor(),
            "stage-6c-advanced-rules"
        );
    }

    #[test]
    fn a_buc_without_business_is_before_stage_zero() {
        let p = &standing(r#"buc BucOrder "Process Order""#)[0];
        assert_eq!(p.reached(), None);
        let next = next_required(p);
        assert_eq!(next.id, StageId::Scope);
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
        let p = &standing(&src)[0];
        assert_eq!(p.view(StageId::EntityStructure).verdict(), Verdict::Met);
        assert_eq!(p.reached(), Some(StageId::DataTouchpoints));
        let next = next_required(p);
        assert_eq!(next.id, StageId::InteractionBoundary);
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
        let p = &standing(src)[0];
        assert_eq!(p.reached(), Some(StageId::Skeleton));
        assert_eq!(
            p.view(StageId::DataTouchpoints).gaps[0].subject_id,
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
        let p = &standing(src)[0];
        assert_eq!(p.view(StageId::Skeleton).verdict(), Verdict::Met);
        assert_eq!(p.view(StageId::DataTouchpoints).verdict(), Verdict::Met);
        assert_eq!(p.view(StageId::InteractionBoundary).verdict(), Verdict::Met);
        assert_eq!(p.reached(), Some(StageId::InteractionBoundary));
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
        let p = &standing(&src)[0];
        let structure = p.view(StageId::EntityStructure);
        assert_eq!(structure.verdict(), Verdict::Open);
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
        let p = &standing(&src)[0];
        assert_eq!(p.reached(), Some(StageId::Lifecycle));
        let guardrails = next_required(p);
        assert_eq!(guardrails.id, StageId::LocalGuardrails);
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
        let p = &standing(&src)[0];
        let lifecycle = next_required(p);
        assert_eq!(lifecycle.id, StageId::Lifecycle);
        assert!(lifecycle.gaps[0].message.contains("status"));
        assert!(!lifecycle.gaps[0].message.contains("note"));
    }
}
