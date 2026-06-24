use rdra_ish_syntax::ast::Kind;
use std::collections::HashMap;

use super::refs::NodeRef;

// ── Symbol table ─────────────────────────────────────────────────────────────

/// Kind name string for a [`NodeRef`] (`"actor"`, `"usecase"`, …).
pub fn node_ref_kind(node: &NodeRef) -> &'static str {
    node_kind_tag(node)
}

fn node_kind_tag(node: &NodeRef) -> &'static str {
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

fn node_ref_matches_kind(node: &NodeRef, kind: &Kind) -> bool {
    matches!(
        (node, kind),
        (NodeRef::Actor(_), Kind::Actor)
            | (NodeRef::ExtSystem(_), Kind::ExtSystem)
            | (NodeRef::System(_), Kind::System)
            | (NodeRef::Requirement(_), Kind::Requirement)
            | (NodeRef::Adr(_), Kind::Adr)
            | (NodeRef::Nfr(_), Kind::Nfr)
            | (NodeRef::Quality(_), Kind::Quality)
            | (NodeRef::Constraint(_), Kind::Constraint)
            | (NodeRef::Concept(_), Kind::Concept)
            | (NodeRef::DomainObject(_), Kind::DomainObject)
            | (NodeRef::Aggregate(_), Kind::Aggregate)
            | (NodeRef::ValueObject(_), Kind::ValueObject)
            | (NodeRef::Business(_), Kind::Business)
            | (NodeRef::Buc(_), Kind::Buc)
            | (NodeRef::Flow(_), Kind::Flow)
            | (NodeRef::Step(_), Kind::Step)
            | (NodeRef::UsageScene(_), Kind::UsageScene)
            | (NodeRef::UseCase(_), Kind::UseCase)
            | (NodeRef::Screen(_), Kind::Screen)
            | (NodeRef::Field(_), Kind::Field)
            | (NodeRef::Event(_), Kind::Event)
            | (NodeRef::Entity(_), Kind::Entity)
            | (NodeRef::State(_), Kind::State)
            | (NodeRef::Condition(_), Kind::Condition)
            | (NodeRef::Variation(_), Kind::Variation)
            | (NodeRef::Api(_), Kind::Api)
            | (NodeRef::Dto(_), Kind::Dto)
            | (NodeRef::Location(_), Kind::Location)
            | (NodeRef::Timing(_), Kind::Timing)
            | (NodeRef::Medium(_), Kind::Medium)
            | (NodeRef::Permission(_), Kind::Permission)
    )
}

/// The result of an unqualified symbol lookup.
pub enum LookupResult<'a> {
    /// Exactly one node with this id.
    Found(&'a NodeRef),
    /// No node with this id.
    NotFound,
    /// Multiple nodes with this id (different kinds). Carries the kind names.
    Ambiguous(Vec<&'static str>),
}

/// Symbol table that allows the same identifier to be reused across different
/// kinds (e.g. `actor Add` and `usecase Add` can coexist).
///
/// Qualified references (`usecase::Add`) resolve unambiguously.
/// Unqualified references (`Add`) succeed only when the identifier is unique
/// across all kinds; otherwise `LookupResult::Ambiguous` is returned.
#[derive(Debug, Default)]
pub struct SymbolTable {
    by_id: HashMap<std::string::String, Vec<NodeRef>>,
}

impl SymbolTable {
    /// Insert a node.
    ///
    /// Returns `true` when a node of the **same kind** with the same id already
    /// exists (duplicate definition error). Cross-kind duplicates are allowed.
    pub fn insert(&mut self, id: std::string::String, node: NodeRef) -> bool {
        let entries = self.by_id.entry(id).or_default();
        if entries
            .iter()
            .any(|n| node_kind_tag(n) == node_kind_tag(&node))
        {
            return true;
        }
        entries.push(node);
        false
    }

    /// Unqualified lookup.
    pub fn lookup(&self, id: &str) -> LookupResult<'_> {
        match self.by_id.get(id) {
            None => LookupResult::NotFound,
            Some(v) if v.len() == 1 => LookupResult::Found(&v[0]),
            Some(v) => LookupResult::Ambiguous(v.iter().map(|n| node_kind_tag(n)).collect()),
        }
    }

    /// Kind-qualified lookup: `kind::id`.
    pub fn lookup_qualified(&self, kind: &Kind, id: &str) -> Option<&NodeRef> {
        self.by_id
            .get(id)?
            .iter()
            .find(|n| node_ref_matches_kind(n, kind))
    }

    /// Namespace-qualified lookup: `a.Foo` → look up the last segment.
    /// Returns `None` when the id is missing or ambiguous.
    pub fn resolve_qref(&self, parts: &[std::string::String]) -> Option<&NodeRef> {
        let id = parts.last()?;
        match self.lookup(id) {
            LookupResult::Found(n) => Some(n),
            _ => None,
        }
    }

    /// Simple presence check by bare id (any kind).
    pub fn get(&self, id: &str) -> Option<&NodeRef> {
        match self.lookup(id) {
            LookupResult::Found(n) => Some(n),
            _ => None,
        }
    }
}
