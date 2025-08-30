use lru::LruCache;
use std::{num::NonZeroUsize, sync::{Arc, Mutex}};
use wasmer::Module;

pub struct ModuleCache {
    in_memory_cache: LruCache<String, Arc<Module>>,
}

impl ModuleCache {
    pub fn new() -> Self {
        Self {
            in_memory_cache: LruCache::new(NonZeroUsize::new(128).unwrap()), // Set a maximum size for the cache
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<Module>> {
        self.in_memory_cache.get(key).cloned()
    }

    pub fn put(&mut self, key: String, module: Arc<Module>) {
        self.in_memory_cache.put(key, module);
    }
}