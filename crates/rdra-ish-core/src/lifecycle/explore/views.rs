//! The distinct view patterns per entity, interned and closed under the moves.

use std::collections::BTreeSet;

use crate::lifecycle::types::ActId;

use super::EntityView;

impl EntityView {
    pub(crate) fn id(&self, key: &[u8]) -> Option<usize> {
        self.index.get(key).copied()
    }

    pub(super) fn intern(&mut self, key: Box<[u8]>) -> usize {
        if let Some(id) = self.index.get(&key) {
            return *id;
        }
        let id = self.patterns.len();
        self.index.insert(key.clone(), id);
        self.patterns.push(key);
        self.initial.push(false);
        self.changes.push(false);
        self.deletable.push(false);
        self.touch.push(BTreeSet::new());
        id
    }

    /// The actions on some path to each pattern: the least set closed under the moves.
    /// It depends only on which moves exist, not on the order BFS found them.
    pub fn via(&self) -> Vec<BTreeSet<ActId>> {
        let mut via = self.touch.clone();
        loop {
            let mut grew = false;
            for &(from, act, to) in &self.moves {
                let mut add = via[from].clone();
                add.insert(act);
                if !add.is_subset(&via[to]) {
                    via[to].extend(add);
                    grew = true;
                }
            }
            if !grew {
                return via;
            }
        }
    }
}
