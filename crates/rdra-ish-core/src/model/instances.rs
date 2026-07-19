use super::refs::ModelColumn;

/// 各要素（全て id: String, label: String を持つ最小構造）
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct ExtSystem {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct System {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub priority: Option<std::string::String>,
    pub sources: Vec<std::string::String>,
    pub stakeholders: Vec<std::string::String>,
    pub owner: Option<std::string::String>,
    pub acceptance_criteria: Vec<std::string::String>,
    pub status: Option<std::string::String>,
    pub risk: Option<std::string::String>,
    pub rationale: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Adr {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub status: Option<std::string::String>,
    pub context: Vec<std::string::String>,
    pub decision: Option<std::string::String>,
    pub consequences: Vec<std::string::String>,
    pub accepted_options: Vec<std::string::String>,
    pub rejected_options: Vec<std::string::String>,
    pub reasons: Vec<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Nfr {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub metric: Option<std::string::String>,
    pub target: Option<std::string::String>,
    pub window: Option<std::string::String>,
    pub slo: Option<std::string::String>,
    pub availability: Option<std::string::String>,
    pub resilience: Option<std::string::String>,
    pub audit: Option<std::string::String>,
    pub logging: Option<std::string::String>,
    pub retention: Option<std::string::String>,
    pub privacy: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Quality {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Constraint {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub metric: Option<std::string::String>,
    pub target: Option<std::string::String>,
    pub window: Option<std::string::String>,
    pub slo: Option<std::string::String>,
    pub availability: Option<std::string::String>,
    pub resilience: Option<std::string::String>,
    pub audit: Option<std::string::String>,
    pub logging: Option<std::string::String>,
    pub retention: Option<std::string::String>,
    pub privacy: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Concept {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct DomainObject {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Aggregate {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct ValueObject {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Business {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Buc {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Flow {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct UsageScene {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct UseCase {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub preconditions: Vec<std::string::String>,
    pub postconditions: Vec<std::string::String>,
    pub guards: Vec<std::string::String>,
    pub alternatives: Vec<std::string::String>,
    pub errors: Vec<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Screen {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub access: Option<std::string::String>,
    pub required: Option<bool>,
    pub source: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub columns: Vec<ModelColumn>,
    /// Primary key column names (single `@pk` or composite `@pk(a, b)`).
    pub primary_key: Vec<std::string::String>,
    pub unique_constraints: Vec<Vec<std::string::String>>,
    pub indexes: Vec<Vec<std::string::String>>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Variation {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Api {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub method: Option<std::string::String>,
    pub path: Option<std::string::String>,
    pub idempotency: Option<std::string::String>,
    pub mode: Option<std::string::String>,
    pub auth_scheme: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Dto {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub fields: Vec<ModelColumn>,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Timing {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Medium {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Permission {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}
