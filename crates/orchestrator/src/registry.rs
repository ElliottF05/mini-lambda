/// Information about a registered Worker.
#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub address: String,
} 

/// Registry to manage the Workers registered to this Orchestrator.
#[derive(Debug)]
pub struct WorkerRegistry {
    inner: Vec<WorkerInfo>,
}

impl WorkerRegistry {
    /// Create a new WorkerRegistry.
    pub fn new() -> Self {
        Self {
            inner: Vec::new(),
        }
    }

    /// Registers a new Worker with the given address.
    pub fn register_worker(&mut self, address: String) {
        let worker_info = WorkerInfo { address };
        self.inner.push(worker_info);
    }

    /// Retrieves a Worker (represented by WorkerInfo) from the registry.
    pub fn get_worker(&self) -> Option<&WorkerInfo> {
        self.inner.get(0)
    }
}