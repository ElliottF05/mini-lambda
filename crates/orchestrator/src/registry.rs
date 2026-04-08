use priority_queue::PriorityQueue;

/// Registry to manage the Workers registered to this Orchestrator.
#[derive(Debug)]
pub struct WorkerRegistry {
    inner: PriorityQueue<String, u32>,
}

impl WorkerRegistry {
    /// Create a new WorkerRegistry.
    pub fn new() -> Self {
        Self {
            inner: PriorityQueue::new()
        }
    }

    /// Registers a new Worker with the given address.
    pub fn register_worker(&mut self, address: String, credits: u32) {
        self.inner.push(address, credits);
    }

    /// Retrieves the Worker address with the most available credits from the registry and 
    /// decrements its credit count by one. Returns None if there are no Workers with 
    /// any available credits.
    pub fn get_worker(&mut self) -> Option<String> {
        if let Some((address, credits)) = self.inner.peek() {
            let (address, credits) = (address.clone(), *credits);
            if credits > 0 {
                self.inner.change_priority(&address, credits - 1);
                Some(address)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Update the credit count for a given worker address in the registry.
    /// Logs an error if the worker isn't in the registry.
    pub fn update_credits(&mut self, worker_address: &str, delta: u32) {
        if !self.inner.change_priority_by(worker_address, |p| *p += delta) {
            tracing::warn!(worker = %worker_address, "attempted to update credits for an unknown worker");
        }
    }

    /// Removes a given worker from the registry. Logs an error if the worker isn't present.
    pub fn deregister_worker(&mut self, worker_address: &str) {
        if self.inner.remove(worker_address).is_none() {
            tracing::warn!(worker = %worker_address, "attempted to remove an unknown worker");
        }
    }

    /// Returns true if any registered Worker has at least one available credit.
    pub fn has_available_credits(&self) -> bool {
        self.inner.peek().map(|(_, &credits)| credits > 0).unwrap_or(false)
    }
}