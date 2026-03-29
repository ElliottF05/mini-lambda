use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{job_queue::JobQueue, registry::WorkerRegistry};

/// Orchestrator struct representing the main Orchestrator server component.
/// It implements CliApi and WorkerApi services, see cli_api.rs and worker_api.rs for details.
/// Orchestrator internally uses Arc<RwLock<_>> so can be cloned cheaply.
#[derive(Debug, Clone)]
pub struct Orchestrator {
    // note: all shared state fields should use Arc<RwLock<...>> for thread safety
    pub registry: Arc<RwLock<WorkerRegistry>>,
    pub job_queue: Arc<RwLock<JobQueue>>,
}

impl Default for Orchestrator {
    /// Creates a new Orchestrator instance.
    fn default() -> Self {
        Self {
            registry: Arc::new(RwLock::new(WorkerRegistry::new())),
            job_queue: Arc::new(RwLock::new(JobQueue::new()))
        }
    }
}