use std::sync::Arc;

use dashmap::DashMap;
use shared::{CreditUpdate, JobState, WorkerMessage, worker_message};
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::worker::Worker;

/// RAII guard that returns a credit via an update to the Orchestrator when dropped
/// and drops resources associated to this job
pub struct JobGuard {
    tx: Sender<WorkerMessage>,
    cancellation_tokens: Arc<DashMap<Uuid, CancellationToken>>,
    job_id: Uuid,
    state: Option<JobState>
}

impl JobGuard {
    /// Creates a new JobGuard bound to the given Worker.
    pub fn new(
        tx: Sender<WorkerMessage>, 
        cancellation_tokens: Arc<DashMap<Uuid, CancellationToken>>,
        job_id: Uuid
    ) -> Self {
        Self { tx, cancellation_tokens, job_id, state: Some(JobState::Failed) }
    }

    pub fn set_completed(&mut self) {
        self.state = Some(JobState::Completed)
    }
    pub fn set_cancelled(&mut self) {
        self.state = None
    }
}

impl Drop for JobGuard {
    /// Sends a credit update to the Orchestrator, returning one credit.
    /// Also drops Worker resources associated with this job
    fn drop(&mut self) {
        if self.cancellation_tokens.remove(&self.job_id).is_none() {
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
        if let Some(job_state) = self.state {
            Worker::send_job_update_to_orchestrator(self.tx.clone(), self.job_id, job_state);
        }
    }
}