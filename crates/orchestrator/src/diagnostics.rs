use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use shared::JobUpdate;
use uuid::Uuid;

// TODO: add simple triple slash docs for all structs, enums, and function in this file.

// TODO: add eviction policy so that only 1000 inactive jobs are held,
// this can also apply to 1000 old workers and clients
#[derive(Debug, Clone, Default)]
pub struct DiagnosticsStore {
    pub jobs: DashMap<Uuid, JobInfo>,
    pub clients: DashMap<String, ClientInfo>,
    pub workers: DashMap<String, WorkerInfo>,
}

impl DiagnosticsStore {
    pub fn handle_worker_job_update(&self, worker_address: &str, job_update: &JobUpdate) {
        let job_id = Uuid::from_slice(&job_update.job_id)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "ERROR: received malformed job_id bytes in job update from worker, this should never happen");
                std::process::exit(1);
            });

        let Some(mut job_info) = self.jobs.get_mut(&job_id) else {
            // TODO: after eviction is added, None here is expected for evicted jobs — return early instead
            tracing::warn!(job_id = %job_id, "job not found in diagnostics store during job update");
            return;
        };
        let Some(mut client_info) = self.clients.get_mut(&job_info.client_address) else {
            tracing::warn!(job_id = %job_id, client = %job_info.client_address, "client not found in diagnostics store during job update");
            return;
        };
        let Some(mut worker_info) = self.workers.get_mut(worker_address) else {
            tracing::warn!(job_id = %job_id, worker = %worker_address, "worker not found in diagnostics store during job update");
            return;
        };

        let received_state: JobState = job_update.state().into();
        if received_state <= job_info.state {
            return;
        }

        let now = SystemTime::now();
        match received_state {
            JobState::Compiling => {
                job_info.compiling_at = Some(now);
            },
            JobState::Executing => {
                job_info.executing_at = Some(now);
                worker_info.jobs_received += 1;
            },
            JobState::Failed | JobState::Completed | JobState::Cancelled => {
                let worker_time = if let Some(compiling_at) = job_info.compiling_at {
                    now.duration_since(compiling_at).unwrap_or_default()
                } else if let Some(executing_at) = job_info.executing_at {
                    now.duration_since(executing_at).unwrap_or_default()
                } else {
                    Duration::ZERO
                };

                client_info.total_worker_time += worker_time;
                worker_info.total_job_time += worker_time;

                job_info.completed_at = Some(now);
            },
            _ => {},
        }
        job_info.state = received_state;
    }

    pub fn handle_client_connected(&self, client_address: &str) {
        let now = SystemTime::now();
        self.clients.entry(client_address.to_string()).or_insert(ClientInfo {
            address: client_address.to_string(),
            jobs_submitted: 0,
            total_queue_time: Duration::ZERO,
            total_worker_time: Duration::ZERO,
            connected_at: now,
            last_seen_at: now,
        });
    }

    pub fn handle_job_enqueue(&self, job_id: Uuid, client_address: &str) {
        let job_info = JobInfo {
            job_id,
            state: JobState::Queued,
            client_address: client_address.to_string(),
            worker_address: None,
            queued_at: SystemTime::now(),
            compiling_at: None,
            executing_at: None,
            completed_at: None
        };
        self.jobs.insert(job_id, job_info);

        let Some(mut client_info) = self.clients.get_mut(client_address) else {
            tracing::warn!(job_id = %job_id, client = %client_address, "client not found in diagnostics store during job enqueue");
            return;
        };
        client_info.last_seen_at = SystemTime::now();
        client_info.jobs_submitted += 1;
    }

    pub fn handle_cancel_queued_job(&self, job_id: Uuid) {
        let Some(mut job_info) = self.jobs.get_mut(&job_id) else {
            tracing::warn!(job_id = %job_id, "job not found in diagnostics store during queued cancel");
            return;
        };
        let Some(mut client_info) = self.clients.get_mut(&job_info.client_address.clone()) else {
            tracing::warn!(job_id = %job_id, client = %job_info.client_address, "client not found in diagnostics store during queued cancel");
            return;
        };

        let now = SystemTime::now();

        job_info.state = JobState::Cancelled;
        job_info.completed_at = Some(now);

        client_info.total_queue_time += now.duration_since(job_info.queued_at).unwrap_or_default();
        client_info.last_seen_at = now;
    }

    pub fn handle_dispatch_job(&self, job_id: Uuid, worker_address: &str) {
        let Some(mut job_info) = self.jobs.get_mut(&job_id) else {
            tracing::warn!(job_id = %job_id, "job not found in diagnostics store during dispatch");
            return;
        };
        let Some(mut client_info) = self.clients.get_mut(&job_info.client_address.clone()) else {
            tracing::warn!(job_id = %job_id, client = %job_info.client_address, "client not found in diagnostics store during dispatch");
            return;
        };

        let now = SystemTime::now();
        job_info.state = JobState::Dispatched;
        job_info.worker_address = Some(worker_address.to_string());
        client_info.total_queue_time += now.duration_since(job_info.queued_at).unwrap_or_default();
    }

    pub fn handle_worker_connected(&self, worker_address: &str) {
        self.workers.insert(worker_address.to_string(), WorkerInfo {
            address: worker_address.to_string(),
            jobs_received: 0,
            total_job_time: Duration::ZERO,
            connected_at: SystemTime::now(),
            disconnected_at: None
        });
    }

    pub fn handle_worker_disconnected(&self, worker_address: &str) {
        let Some(mut worker_info) = self.workers.get_mut(worker_address) else {
            tracing::warn!(worker = %worker_address, "worker not found in diagnostics store during disconnect");
            return;
        };
        worker_info.disconnected_at = Some(SystemTime::now());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobState {
    Queued,
    Dispatched,
    Compiling,
    Executing,
    Failed,
    Completed,
    Cancelled,
}

impl From<shared::JobState> for JobState {
    fn from(value: shared::JobState) -> Self {
        match value {
            shared::JobState::Compiling => JobState::Compiling,
            shared::JobState::Executing => JobState::Executing,
            shared::JobState::Failed => JobState::Failed,
            shared::JobState::Completed => JobState::Completed,
            shared::JobState::Cancelled => JobState::Cancelled,
            shared::JobState::Unspecified => {
                tracing::error!("ERROR: received JobState::Unspecified from worker, this should never happen");
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub job_id: Uuid,
    pub state: JobState,
    pub client_address: String,
    pub worker_address: Option<String>,
    pub queued_at: SystemTime,
    pub compiling_at: Option<SystemTime>,
    pub executing_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub address: String,
    pub jobs_submitted: u32,
    pub total_queue_time: Duration,
    pub total_worker_time: Duration,
    pub connected_at: SystemTime,
    pub last_seen_at: SystemTime
}

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub address: String,
    pub jobs_received: u32,
    pub total_job_time: Duration,
    pub connected_at: SystemTime,
    pub disconnected_at: Option<SystemTime>
}
