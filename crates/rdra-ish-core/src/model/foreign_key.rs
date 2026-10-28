//! The key facts `relate` yields. `analysis::fk` writes them once; every consumer reads them.

use super::{EntityKey, SemanticModel};
use crate::location::LocatedSpan;

/// Index into `SemanticModel::foreign_keys`. Stable for the life of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FkId(u32);

impl FkId {
    pub(crate) fn at(index: usize) -> Self {
        FkId(u32::try_from(index).expect("fewer than 2^32 foreign keys"))
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// How many rows one side of a key relates to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Multiplicity {
    pub min: Min,
    pub max: Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Min {
    Zero,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Max {
    One,
    Many,
}

/// `.on_delete(..)` / `.on_update(..)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefAction {
    Cascade,
    Restrict,
    SetNull,
    SetDefault,
    NoAction,
}

impl RefAction {
    pub const ALL: [RefAction; 5] = [
        RefAction::Cascade,
        RefAction::Restrict,
        RefAction::SetNull,
        RefAction::SetDefault,
        RefAction::NoAction,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == text)
    }

    /// The spelling the DSL accepts, which CSV, JSON Schema and `list` print.
    pub fn as_str(self) -> &'static str {
        match self {
            RefAction::Cascade => "cascade",
            RefAction::Restrict => "restrict",
            RefAction::SetNull => "set_null",
            RefAction::SetDefault => "set_default",
            RefAction::NoAction => "no_action",
        }
    }
}

/// The one key fact of a relate whose child holds the key (N:1, 1:N, 1:1). An N:M with
/// `.via(J)` yields its two facts through the junction's N:1 relates.
#[derive(Clone, Debug)]
pub struct ForeignKey {
    pub child: EntityKey,
    pub parent: EntityKey,
    /// `(child column, parent key column)` pairs. Empty when `analysis::fk` rejected the key
    /// (`FkConflict`, `MissingPk`, `CompositePkFkUnsupported`).
    pub columns: Vec<(String, String)>,
    /// Children per parent: `max == One` is a 1:1.
    pub children: Multiplicity,
    /// Parents per child: `Zero` is `.optional()`, a nullable key.
    pub parent_min: Min,
    pub on_delete: Option<RefAction>,
    pub on_update: Option<RefAction>,
    /// Index into `SemanticModel::relations` of the relate that yields it.
    pub relation: usize,
    pub site: Option<LocatedSpan>,
}

impl ForeignKey {
    pub fn optional(&self) -> bool {
        self.parent_min == Min::Zero
    }

    pub fn unique(&self) -> bool {
        self.children.max == Max::One
    }
}

/// A set of columns no two rows of an entity share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniqueKey {
    pub columns: Vec<String>,
    pub source: UniqueSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniqueSource {
    /// `@unique(a, b)` on the entity.
    Declared,
    /// The two key columns of an N:M junction.
    Junction(FkId, FkId),
}

impl SemanticModel {
    pub fn foreign_keys(&self) -> impl Iterator<Item = (FkId, &ForeignKey)> {
        self.foreign_keys
            .iter()
            .enumerate()
            .map(|(i, fk)| (FkId::at(i), fk))
    }

    pub fn foreign_key(&self, id: FkId) -> &ForeignKey {
        &self.foreign_keys[id.index()]
    }

    /// The key a column of `entity` belongs to, if it is a key column.
    pub fn column_fk(&self, entity: EntityKey, column: &str) -> Option<(FkId, &ForeignKey)> {
        self.foreign_keys().find(|(_, fk)| {
            fk.child == entity && fk.columns.iter().any(|(child, _)| child == column)
        })
    }

    /// Composite unique sets of `entity`: the declared ones, then each junction pair that
    /// no declared set already covers. Single-column `@unique` stays on the column.
    pub fn unique_keys(&self, entity: EntityKey) -> Vec<UniqueKey> {
        let mut keys: Vec<UniqueKey> = self.entities[entity]
            .unique_constraints
            .iter()
            .map(|columns| UniqueKey {
                columns: columns.clone(),
                source: UniqueSource::Declared,
            })
            .collect();
        for &(junction, a, b) in &self.junction_keys {
            if junction != entity {
                continue;
            }
            let mut columns: Vec<String> = [a, b]
                .iter()
                .flat_map(|&id| self.foreign_key(id).columns.iter().map(|(c, _)| c.clone()))
                .collect();
            columns.sort();
            let covered = keys.iter().any(|key| {
                let mut declared = key.columns.clone();
                declared.sort();
                declared == columns
            });
            if !covered {
                keys.push(UniqueKey {
                    columns,
                    source: UniqueSource::Junction(a, b),
                });
            }
        }
        keys
    }
}
