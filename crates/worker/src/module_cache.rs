use lru::LruCache;
use std::{num::NonZeroUsize, sync::{Arc, Mutex}};
use wasmer::Module;

pub struct ModuleCache<T = Module> {
    in_memory_cache: LruCache<String, Arc<T>>,
}

impl<T> ModuleCache<T> {
    pub fn new_with_capacity(cap: NonZeroUsize) -> Self {
        Self {
            in_memory_cache: LruCache::new(cap),
        }
    }

    pub fn new() -> Self {
        Self {
            in_memory_cache: LruCache::new(NonZeroUsize::new(128).unwrap()), // Set a maximum size for the cache
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<T>> {
        self.in_memory_cache.get(key).cloned()
    }

    pub fn put(&mut self, key: String, module: Arc<T>) {
        self.in_memory_cache.put(key, module);
    }
}


// Unit tests for ModuleCache
#[cfg(test)]
mod tests {
    use super::*;
    use std::{num::NonZero, sync::Arc};

    // Use the generic cache with a lightweight test type (String) to avoid compiling wasm.
    type TestCache = ModuleCache<String>;

    #[test]
    fn put_and_get_returns_same_arc() {
        let mut cache = TestCache::new_with_capacity(NonZero::new(4).unwrap());
        let key = "modA".to_string();
        let value = Arc::new("module-a".to_string());
        cache.put(key.clone(), value.clone());

        let got = cache.get(&key).expect("expected to find module");
        // Arc::ptr_eq verifies we got the same allocation back
        assert!(Arc::ptr_eq(&got, &value));
    }

    #[test]
    fn lru_eviction_happens_when_capacity_exceeded() {
        // small capacity to exercise eviction
        let mut cache = TestCache::new_with_capacity(NonZero::new(2).unwrap());
        let a = Arc::new("A".to_string());
        let b = Arc::new("B".to_string());
        let c = Arc::new("C".to_string());

        cache.put("a".to_string(), a.clone());
        cache.put("b".to_string(), b.clone());

        // access "a" so "b" becomes LRU
        let _ = cache.get("a");

        // inserting c should evict "b"
        cache.put("c".to_string(), c.clone());

        assert!(cache.get("a").is_some(), "recently used entry should remain");
        assert!(cache.get("b").is_none(), "least-recently-used entry should be evicted");
        assert!(cache.get("c").is_some(), "new entry should be present");
    }

    #[test]
    fn new_with_capacity_respects_capacity() {
        let cap = 3;
        let mut cache = TestCache::new_with_capacity(NonZero::new(cap).unwrap());
        for i in 0..cap {
            cache.put(format!("k{}", i), Arc::new(format!("v{}", i)));
        }
        // adding one more should evict one entry
        cache.put("extra".to_string(), Arc::new("v_extra".to_string()));

        // total entries should not exceed capacity
        let mut found = 0;
        for i in 0..cap {
            if cache.get(&format!("k{}", i)).is_some() {
                found += 1;
            }
        }
        if cache.get("extra").is_some() {
            found += 1;
        }
        assert!(found <= cap, "cache should not exceed configured capacity");
    }
}