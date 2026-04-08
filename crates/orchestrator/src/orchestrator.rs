use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{diagnostics::DiagnosticsStore, job_queue::JobQueue, registry::WorkerRegistry};

/// Orchestrator struct representing the main Orchestrator server component.
/// It implements CliApi and WorkerApi services, see cli_api.rs and worker_api.rs for details.
/// Orchestrator internally uses Arc<RwLock/Mutex<_>> so can be cloned cheaply.
#[derive(Debug, Clone)]
pub struct Orchestrator {
    // note: all shared state fields should use Arc<RwLock/Mutex<...>> for thread safety
    pub registry: Arc<Mutex<WorkerRegistry>>,
    pub job_queue: Arc<Mutex<JobQueue>>,
    pub worker_password: Option<String>,
    pub client_password: Option<String>,
    pub jwt_secret: [u8; 32],
    pub network_access_allowed: bool,

    // diagnostics
    pub diagnostics: Arc<DiagnosticsStore>,
}

impl Orchestrator {
    /// Creates a new Orchestrator instance
    pub fn new(worker_password: Option<String>, client_password: Option<String>, network_access_allowed: bool) -> Self {
        Self {
            registry: Arc::new(Mutex::new(WorkerRegistry::new())),
            job_queue: Arc::new(Mutex::new(JobQueue::new())),
            worker_password,
            client_password,
            jwt_secret: rand::random(),
            diagnostics: Arc::new(DiagnosticsStore::new()),
            network_access_allowed
        }
    }
}