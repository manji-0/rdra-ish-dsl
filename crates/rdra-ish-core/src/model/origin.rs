use slotmap::{Key, SecondaryMap, SlotMap};
use std::ops::{Index, IndexMut};

use super::keys::ExtSystemKey;
use super::PublishMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeignWhat {
    UseCase,
    Event,
    Permission,
    PublishedEntity(PublishMode),
    /// An internal entity the owner consented to share; every use is `DirectEntityReference`.
    SharedEntity,
}

impl ForeignWhat {
    pub fn kind(self) -> &'static str {
        match self {
            ForeignWhat::UseCase => "usecase",
            ForeignWhat::Event => "event",
            ForeignWhat::Permission => "permission",
            ForeignWhat::PublishedEntity(_) | ForeignWhat::SharedEntity => "entity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The bound external system whose qualifier the stub's id starts with.
    pub ext: ExtSystemKey,
    /// The `[dependencies]` key of the manifest.
    pub dep: String,
    /// The id in the owner's model, without the qualifier.
    pub foreign_id: String,
    pub what: ForeignWhat,
}

pub fn slug(qualified: &str) -> String {
    qualified.replace('.', "__")
}

/// A slot map whose default views see local elements only. `get`, indexing, and
/// `contains_key` take any key, so a relation endpoint may be a foreign stub. `foreign` is
/// the opt-in view.
#[derive(Debug)]
pub struct Partitioned<K: Key, V> {
    all: SlotMap<K, V>,
    foreign: SecondaryMap<K, Origin>,
}

impl<K: Key, V> Default for Partitioned<K, V> {
    fn default() -> Self {
        Partitioned {
            all: SlotMap::with_key(),
            foreign: SecondaryMap::new(),
        }
    }
}

impl<K: Key, V> Partitioned<K, V> {
    pub fn insert(&mut self, value: V) -> K {
        self.all.insert(value)
    }

    pub fn insert_foreign(&mut self, value: V, origin: Origin) -> K {
        let key = self.all.insert(value);
        self.foreign.insert(key, origin);
        key
    }

    pub fn get(&self, key: K) -> Option<&V> {
        self.all.get(key)
    }

    pub fn contains_key(&self, key: K) -> bool {
        self.all.contains_key(key)
    }

    pub fn origin(&self, key: K) -> Option<&Origin> {
        self.foreign.get(key)
    }

    fn is_local(&self, key: K) -> bool {
        self.foreign.is_empty() || !self.foreign.contains_key(key)
    }

    /// Local elements in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> + '_ {
        self.all.iter().filter(move |(k, _)| self.is_local(*k))
    }

    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, v)| v)
    }

    /// The number of local elements.
    pub fn len(&self) -> usize {
        self.all.len() - self.foreign.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The foreign stubs, each with its origin.
    pub fn foreign(&self) -> impl Iterator<Item = (K, &V, &Origin)> + '_ {
        self.foreign
            .iter()
            .map(move |(k, origin)| (k, &self.all[k], origin))
    }
}

/// Local elements only, like `iter`.
impl<'a, K: Key, V> IntoIterator for &'a Partitioned<K, V> {
    type Item = (K, &'a V);
    type IntoIter = Box<dyn Iterator<Item = (K, &'a V)> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<K: Key, V> Index<K> for Partitioned<K, V> {
    type Output = V;
    fn index(&self, key: K) -> &V {
        &self.all[key]
    }
}

impl<K: Key, V> IndexMut<K> for Partitioned<K, V> {
    fn index_mut(&mut self, key: K) -> &mut V {
        &mut self.all[key]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EntityKey, PublishMode};

    fn origin() -> Origin {
        Origin {
            ext: ExtSystemKey::default(),
            dep: "auth".into(),
            foreign_id: "SessionView".into(),
            what: ForeignWhat::PublishedEntity(PublishMode::Upsert),
        }
    }

    #[test]
    fn default_views_see_local_elements_only() {
        let mut map: Partitioned<EntityKey, &str> = Partitioned::default();
        let local = map.insert("Order");
        let stub = map.insert_foreign("Auth.SessionView", origin());

        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
        assert_eq!(map.iter().map(|(k, _)| k).collect::<Vec<_>>(), vec![local]);
        assert_eq!(map.values().copied().collect::<Vec<_>>(), vec!["Order"]);
        assert_eq!(map.keys().collect::<Vec<_>>(), vec![local]);
        assert_eq!(map.get(stub), Some(&"Auth.SessionView"));
        assert_eq!(map[stub], "Auth.SessionView");
        assert!(map.contains_key(stub));
        assert_eq!(map.origin(stub), Some(&origin()));
        assert_eq!(map.origin(local), None);
        assert_eq!(
            map.foreign().map(|(k, _, _)| k).collect::<Vec<_>>(),
            vec![stub]
        );
    }

    #[test]
    fn a_map_with_only_stubs_is_empty() {
        let mut map: Partitioned<EntityKey, &str> = Partitioned::default();
        map.insert_foreign("Auth.SessionView", origin());
        assert!(map.is_empty());
        assert_eq!(map.iter().count(), 0);
    }

    #[test]
    fn slug_replaces_the_qualifier_dot() {
        assert_eq!(slug("Auth.SessionView"), "Auth__SessionView");
        assert_eq!(slug("Order"), "Order");
    }
}
