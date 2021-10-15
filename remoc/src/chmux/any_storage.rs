//! Arbitrary data storage.

use std::{any::Any, collections::HashMap, fmt, sync::Arc};
use uuid::Uuid;

/// Box containing any value that is Send, Sync and static.
pub type AnyBox = Box<dyn Any + Send + Sync + 'static>;

/// An entry in [AnyStorage].
pub type AnyEntry = Arc<tokio::sync::RwLock<Option<AnyBox>>>;

type AnyMap = HashMap<Uuid, AnyEntry>;

/// Stores arbitrary data indexed by automatically generated keys.
///
/// Clones share the underlying storage.
#[derive(Clone)]
pub struct AnyStorage {
    entries: Arc<std::sync::Mutex<AnyMap>>,
}

impl fmt::Debug for AnyStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let entries = self.entries.lock().unwrap();
        write!(f, "{:?}", *entries)
    }
}

impl AnyStorage {
    /// Creates a new storage.
    pub(crate) fn new() -> Self {
        Self { entries: Arc::new(std::sync::Mutex::new(AnyMap::new())) }
    }

    /// Insert a new entry into the storage and return its key.
    ///
    /// # Panics
    /// Panics when a duplicate UUID is generated and inserted into the storage.
    /// The probability of this is happening is extremely low.
    pub fn insert(&self, entry: AnyEntry) -> Uuid {
        let key = Uuid::new_v4();
        let mut entries = self.entries.lock().unwrap();
        if entries.insert(key, entry).is_some() {
            panic!("duplicate UUID");
        }
        key
    }

    /// Returns the value from the storage for the specified key.
    pub fn get(&self, key: Uuid) -> Option<AnyEntry> {
        let entries = self.entries.lock().unwrap();
        entries.get(&key).cloned()
    }

    /// Removes the value for the specified key from the storage and returns it.
    pub fn remove(&self, key: Uuid) -> Option<AnyEntry> {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(&key)
    }
}
