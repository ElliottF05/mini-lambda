use priority_queue::PriorityQueue;

/// Registry to manage the Workers registered to this Orchestrator.
#[derive(Debug)]
pub struct WorkerRegistry {
    inner: PriorityQueue<u32, String>,
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
        self.inner.push(credits, address);
    }

    /// Retrieves the Worker address with the most available credits from the registry and 
    /// decrements its credit count by one. Returns None if there are no Workers with 
    /// any available credits.
    pub fn get_worker(&mut self) -> Option<String> {
        if let Some((credits, address)) = self.inner.peek_mut() && *credits > 0 {
            *credits -= 1;
            Some(address.clone())
        } else {
            None
        }
    }
}