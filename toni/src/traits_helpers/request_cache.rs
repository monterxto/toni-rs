use rustc_hash::FxHashMap;
use std::{
    any::{Any, TypeId},
    sync::{Arc, Mutex},
};

/// Per-request instance cache for request-scoped providers.
///
/// Created once at the start of each request handler invocation and threaded
/// through all `Provider::execute` calls via `ProviderContext::Http`. Ensures
/// that multiple services injecting the same request-scoped type receive the
/// same instance within a single request, without a global registry.
pub struct RequestCache {
    inner: Mutex<FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl RequestCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(FxHashMap::default()),
        }
    }

    /// Returns a clone of the cached instance for `T`, if one exists.
    pub fn get<T: Any + Clone + Send + Sync>(&self) -> Option<T> {
        let map = self.inner.lock().unwrap();
        map.get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }

    /// Stores `value` in the cache under `T`'s `TypeId`.
    pub fn insert<T: Any + Clone + Send + Sync>(&self, value: T) {
        let mut map = self.inner.lock().unwrap();
        map.insert(
            TypeId::of::<T>(),
            Arc::new(value) as Arc<dyn Any + Send + Sync>,
        );
    }
}
