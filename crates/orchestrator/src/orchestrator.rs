use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{job_queue::JobQueue, registry::WorkerRegistry};

/// Orchestrator struct representing the main Orchestrator server component.
/// It implements CliApi and WorkerApi services, see cli_api.rs and worker_api.rs for details.
/// Orchestrator internally uses Arc<RwLock/Mutex<_>> so can be cloned cheaply.
#[derive(Debug, Clone)]
pub struct Orchestrator {
    // note: all shared state fields should use Arc<RwLock/Mutex<...>> for thread safety
    pub registry: Arc<Mutex<WorkerRegistry>>,
    pub job_queue: Arc<Mutex<JobQueue>>,
}

impl Default for Orchestrator {
    /// Creates a new Orchestrator instance.
    fn default() -> Self {
        Self {
            registry: Arc::new(Mutex::new(WorkerRegistry::new())),
            job_queue: Arc::new(Mutex::new(JobQueue::new()))
        }
    }
}