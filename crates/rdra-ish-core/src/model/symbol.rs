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
    /// Multiple nodes with this id (different kinds or modules). Carries labels.
    Ambiguous(Vec<&'static str>),
}

/// One registered symbol, optionally tagged with its declaring module path.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub node: NodeRef,
    /// `module shared.actors` path, if the declaring file had a module decl.
    pub module: Option<String>,
}

/// Symbol table that allows the same identifier to be reused across different
/// kinds and across different modules.
///
/// Qualified references (`usecase::Add`) resolve when unique for that kind.
/// Module-scoped lookup disambiguates same-kind ids from different modules.
#[derive(Debug, Default)]
pub struct SymbolTable {
    by_id: HashMap<std::string::String, Vec<SymbolEntry>>,
}

impl SymbolTable {
    /// Insert a node.
    ///
    /// Returns `true` when a node of the **same kind** with the same id already
    /// exists in the **same module** (or both lack a module). Cross-kind and
    /// cross-module duplicates are allowed.
    pub fn insert(&mut self, id: std::string::String, node: NodeRef) -> bool {
        self.insert_in_module(id, node, None)
    }

    pub fn insert_in_module(
        &mut self,
        id: std::string::String,
        node: NodeRef,
        module: Option<String>,
    ) -> bool {
        let entries = self.by_id.entry(id).or_default();
        let kind = node_kind_tag(&node);
        if entries
            .iter()
            .any(|e| node_kind_tag(&e.node) == kind && e.module == module)
        {
            return true;
        }
        entries.push(SymbolEntry { node, module });
        false
    }

    /// Unqualified lookup across all modules/kinds.
    pub fn lookup(&self, id: &str) -> LookupResult<'_> {
        match self.by_id.get(id) {
            None => LookupResult::NotFound,
            Some(v) if v.len() == 1 => LookupResult::Found(&v[0].node),
            Some(v) => LookupResult::Ambiguous(v.iter().map(|e| node_kind_tag(&e.node)).collect()),
        }
    }

    /// Look up `id` preferring a specific module path.
    pub fn lookup_in_module(&self, id: &str, module: &str) -> Option<&NodeRef> {
        let entries = self.by_id.get(id)?;
        let matches: Vec<_> = entries
            .iter()
            .filter(|e| e.module.as_deref() == Some(module))
            .collect();
        match matches.as_slice() {
            [only] => Some(&only.node),
            [] => {
                // Fall back to unique global match (e.g. synthetic states).
                if entries.len() == 1 {
                    Some(&entries[0].node)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Kind-qualified lookup: `kind::id`.
    /// Prefer a unique match; if multiple modules define the same kind+id, None.
    pub fn lookup_qualified(&self, kind: &Kind, id: &str) -> Option<&NodeRef> {
        let entries = self.by_id.get(id)?;
        let matches: Vec<_> = entries
            .iter()
            .filter(|e| node_ref_matches_kind(&e.node, kind))
            .collect();
        match matches.as_slice() {
            [only] => Some(&only.node),
            _ => None,
        }
    }

    /// Kind + module disambiguation.
    pub fn lookup_qualified_in_module(
        &self,
        kind: &Kind,
        id: &str,
        module: &str,
    ) -> Option<&NodeRef> {
        self.by_id.get(id)?.iter().find_map(|e| {
            (e.module.as_deref() == Some(module) && node_ref_matches_kind(&e.node, kind))
                .then_some(&e.node)
        })
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
