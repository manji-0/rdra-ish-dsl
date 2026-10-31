//! What one BUC holds, read once from the model relations.

use std::collections::BTreeSet;

use crate::model::{BucKey, ColumnType, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

pub(crate) struct BucFacts<'m> {
    pub model: &'m SemanticModel,
    pub buc: BucKey,
    pub businesses: Vec<String>,
    pub performs_or_triggers: usize,
    pub buc_triggers: Vec<String>,
    pub use_cases: Vec<UseCaseKey>,
    pub entities: Vec<EntityKey>,
    pub moved: Vec<MovedColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MovedColumn {
    pub entity: EntityKey,
    pub column: String,
    pub axis: Axis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    State,
    NonState,
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
        let axis_of = |entity: EntityKey, column: &str| {
            let state = model.entities[entity].columns.iter().any(|col| {
                col.name == column && matches!(col.col_type, ColumnType::Enum(_) | ColumnType::Bool)
            });
            if state {
                Axis::State
            } else {
                Axis::NonState
            }
        };
        let mut moved: Vec<MovedColumn> = Vec::new();
        let transitions = model
            .state_transitions
            .iter()
            .map(|t| (t.usecase, t.entity, t.column.clone(), Axis::State));
        let effects = model.column_effects.iter().map(|e| {
            (
                e.origin,
                e.entity,
                e.column.clone(),
                axis_of(e.entity, &e.column),
            )
        });
        let propositions = model.proposition_effects.iter().flat_map(|e| {
            let mut columns = vec![e.prop.lhs_column.clone()];
            if let crate::model::CmpRhs::Column(rhs) = &e.prop.rhs {
                columns.push(rhs.clone());
            }
            columns
                .into_iter()
                .map(move |column| (e.origin, e.entity, column, Axis::NonState))
        });
        for (uc, entity, column, axis) in transitions.chain(effects).chain(propositions) {
            if !use_cases.contains(&uc) {
                continue;
            }
            if let Some(known) = moved
                .iter_mut()
                .find(|m| m.entity == entity && m.column == column)
            {
                if axis == Axis::State {
                    known.axis = Axis::State;
                }
            } else {
                moved.push(MovedColumn {
                    entity,
                    column,
                    axis,
                });
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
        let is_trigger = |rel: &crate::model::Relation| {
            rel.kind == RelKind::Triggers
                && matches!(rel.from, NodeRef::Event(_) | NodeRef::Timer(_))
        };
        let performs_or_triggers = model
            .relations
            .iter()
            .filter(|rel| match rel.kind {
                RelKind::Performs => enters(&rel.to),
                RelKind::Triggers => is_trigger(rel) && enters(&rel.to),
                _ => false,
            })
            .count();
        let buc_triggers = model
            .relations
            .iter()
            .filter(|rel| is_trigger(rel) && rel.to == NodeRef::Buc(buc))
            .filter_map(|rel| match rel.from {
                NodeRef::Event(key) => Some(model.events[key].id.clone()),
                NodeRef::Timer(key) => Some(model.timers[key].id.clone()),
                _ => None,
            })
            .collect();
        Self {
            model,
            buc,
            businesses,
            performs_or_triggers,
            buc_triggers,
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

    pub fn touches(&self, uc: UseCaseKey) -> bool {
        !use_case_entities(self.model, uc).is_empty()
    }

    pub fn has_boundary(&self, uc: UseCaseKey) -> bool {
        let node = NodeRef::UseCase(uc);
        self.model.relations.iter().any(|rel| match rel.kind {
            RelKind::Displays | RelKind::Invokes => rel.from == node,
            RelKind::Triggers => rel.to == node,
            _ => false,
        })
    }

    pub fn has_access_vocabulary(&self) -> bool {
        !self.model.permissions.is_empty() || !self.model.media.is_empty()
    }

    pub fn requires_access(&self, uc: UseCaseKey) -> bool {
        let node = NodeRef::UseCase(uc);
        let apis: Vec<&NodeRef> = self
            .model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Invokes && rel.from == node)
            .map(|rel| &rel.to)
            .collect();
        self.model.relations.iter().any(|rel| {
            matches!(
                rel.kind,
                RelKind::RequiresPermission | RelKind::RequiresMedium
            ) && (rel.from == node || apis.contains(&&rel.from))
        })
    }

    pub fn has_attribute(&self, entity: EntityKey) -> bool {
        self.model.entities[entity]
            .columns
            .iter()
            .any(|col| !col.is_pk && self.model.column_fk(entity, &col.name).is_none())
    }

    pub fn related_to_touched(&self, entity: EntityKey) -> bool {
        self.model.foreign_keys().any(|(_, fk)| {
            (fk.child == entity && fk.parent != entity && self.entities.contains(&fk.parent))
                || (fk.parent == entity && fk.child != entity && self.entities.contains(&fk.child))
        })
    }

    pub fn moved_entities(&self) -> Vec<EntityKey> {
        let mut entities = Vec::new();
        for m in &self.moved {
            if !entities.contains(&m.entity) {
                entities.push(m.entity);
            }
        }
        entities
    }

    pub fn moved_columns(&self) -> BTreeSet<(EntityKey, String)> {
        self.moved
            .iter()
            .map(|m| (m.entity, m.column.clone()))
            .collect()
    }

    pub fn column_label(&self, entity: EntityKey, column: &str) -> String {
        format!("{}.{column}", self.entity_id(entity))
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
            if !entities.contains(&entity) && !model.is_foreign(&rel.to) {
                entities.push(entity);
            }
        }
    }
    entities
}
