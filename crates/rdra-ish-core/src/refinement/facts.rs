//! What one BUC holds, read once from the model relations. Checks are pure functions of it.

use crate::model::{BucKey, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

pub(crate) struct BucFacts<'m> {
    pub model: &'m SemanticModel,
    pub buc: BucKey,
    /// Businesses the BUC `belongs` to.
    pub businesses: Vec<String>,
    /// `performs` and event or timer `triggers` that start the BUC or one of its use cases.
    pub starters: usize,
    /// The use cases the BUC `contains`, and those its flows hold through `contains` and
    /// `covers`, in first-reached order.
    pub use_cases: Vec<UseCaseKey>,
    /// Entities the BUC's use cases create, read, update, or delete, directly or through an
    /// invoked API, in first-touch order.
    pub entities: Vec<EntityKey>,
    /// Entities some use case of the BUC moves with `transitions` or `sets`.
    pub moved: Vec<EntityKey>,
}

impl<'m> BucFacts<'m> {
    pub fn collect(model: &'m SemanticModel, buc: BucKey) -> Self {
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
        let businesses = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Belongs && rel.from == NodeRef::Buc(buc))
            .filter_map(|rel| match rel.to {
                NodeRef::Business(key) => Some(model.businesses[key].id.clone()),
                _ => None,
            })
            .collect();
        let enters = |to: &NodeRef| match to {
            NodeRef::Buc(key) => *key == buc,
            NodeRef::UseCase(key) => use_cases.contains(key),
            _ => false,
        };
        let starters = model
            .relations
            .iter()
            .filter(|rel| match rel.kind {
                RelKind::Performs => enters(&rel.to),
                RelKind::Triggers => {
                    matches!(rel.from, NodeRef::Event(_) | NodeRef::Timer(_)) && enters(&rel.to)
                }
                _ => false,
            })
            .count();
        Self {
            model,
            buc,
            businesses,
            starters,
            use_cases,
            entities,
            moved,
        }
    }

    pub fn buc_id(&self) -> &'m str {
        &self.model.bucs[self.buc].id
    }

    pub fn use_case_id(&self, uc: UseCaseKey) -> &'m str {
        &self.model.use_cases[uc].id
    }

    pub fn entity_id(&self, entity: EntityKey) -> &'m str {
        &self.model.entities[entity].id
    }

    /// The use case has a CRUD touchpoint, directly or through an invoked API.
    pub fn touches(&self, uc: UseCaseKey) -> bool {
        !use_case_entities(self.model, uc).is_empty()
    }

    /// The use case works through a screen or an API, or an event or timer runs it.
    pub fn has_boundary(&self, uc: UseCaseKey) -> bool {
        let node = NodeRef::UseCase(uc);
        self.model.relations.iter().any(|rel| match rel.kind {
            RelKind::Displays | RelKind::Invokes => rel.from == node,
            RelKind::Triggers => rel.to == node,
            _ => false,
        })
    }

    /// The entity has an authored column that is neither its primary key nor a foreign key.
    pub fn has_attribute(&self, entity: EntityKey) -> bool {
        self.model.entities[entity]
            .columns
            .iter()
            .any(|col| !col.is_pk && self.model.column_fk(entity, &col.name).is_none())
    }
}

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
