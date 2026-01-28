use std::sync::Arc;

use tokio::sync::RwLock;

use crate::registry::WorkerRegistry;

/// Orchestrator struct representing the main Orchestrator server component.
/// It implements CliApi and WorkerApi services, see cli_api.rs and worker_api.rs for details.
#[derive(Debug, Clone)]
pub struct Orchestrator {
    // note: all shared state fields should use Arc<RwLock<...>> for thread safety
    registry: Arc<RwLock<WorkerRegistry>>,
}

impl Default for Orchestrator {
    /// Creates a new Orchestrator instance.
    fn default() -> Self {
        Self {
            registry: Arc::new(RwLock::new(WorkerRegistry::new())),
        }
    }
}