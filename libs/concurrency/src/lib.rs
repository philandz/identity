use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConcurrencyError {
    #[error("lock poisoned")]
    LockPoisoned,
}

#[derive(Clone, Debug, Default)]
pub struct SyncMap<K, V> {
    inner: Arc<RwLock<HashMap<K, V>>>,
}

impl<K, V> SyncMap<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    pub fn insert(&self, key: K, value: V) -> Result<(), ConcurrencyError> {
        self.inner
            .write()
            .map_err(|_| ConcurrencyError::LockPoisoned)?
            .insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &K) -> Result<Option<V>, ConcurrencyError> {
        Ok(self
            .inner
            .read()
            .map_err(|_| ConcurrencyError::LockPoisoned)?
            .get(key)
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_map_insert_get() {
        let map = SyncMap::<String, i32>::default();
        map.insert("a".to_string(), 1).expect("insert");
        let got = map.get(&"a".to_string()).expect("get");
        assert_eq!(got, Some(1));
    }
}
