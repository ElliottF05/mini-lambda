use std::sync::Arc;

use dashmap::DashMap;
use shared::{CreditUpdate, WorkerMessage, worker_message};
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// RAII guard that returns a credit via an update to the Orchestrator when dropped
/// and drops resources associated to this job
pub struct JobGuard {
    tx: Sender<WorkerMessage>,
    cancellation_tokens: Arc<DashMap<Uuid, CancellationToken>>,
    job_id: Uuid,
}

impl JobGuard {
    /// Creates a new JobGuard bound to the given Worker.
    pub fn new(
        tx: Sender<WorkerMessage>, 
        cancellation_tokens: Arc<DashMap<Uuid, CancellationToken>>,
        job_id: Uuid
    ) -> Self {
        Self { tx, cancellation_tokens, job_id }
    }
}

impl Drop for JobGuard {
    /// Sends a credit update to the Orchestrator, returning one credit.
    /// Also drops Worker resources associated with this job
    fn drop(&mut self) {
        if let Some((_, cancel)) = self.cancellation_tokens.remove(&self.job_id) {
            cancel.cancel(); // Cancel again here for redundancy
        } else {
            tracing::error!(job_id = %self.job_id, "ERROR: missing cancellation token in job guard, this should never happen");
            std::process::exit(1);
        }
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if tx.send(WorkerMessage {
                message: Some(worker_message::Message::CreditUpdate(CreditUpdate { delta: 1 }))
            }).await.is_err() {
                tracing::error!("lost connection to the orchestrator, shutting down");
                std::process::exit(1);
            }
        });
    }
}