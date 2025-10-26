use std::{collections::VecDeque, sync::Arc, time::SystemTime};

use mini_lambda_proto::JobSummary;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

#[derive(Debug)]
pub struct Job {
    pub job_id: Uuid,
    pub submitted_at: SystemTime,
    pub responder: oneshot::Sender<String>,
}

#[derive(Clone)]
pub struct PendingQueue {
    inner: Arc<Mutex<VecDeque<Job>>>,
    max_size: usize,
}

impl PendingQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            max_size,
        }
    }

    pub async fn enqueue(&self, job: Job) -> Result<(), ()> {
        let mut q = self.inner.lock().await;
        if q.len() < self.max_size {
            q.push_back(job);
            Ok(())
        } else {
            Err(())
        }
    }

    pub async fn dequeue(&self) -> Option<Job> {
        let mut q = self.inner.lock().await;
        q.pop_front()
    }

    pub async fn remove_job_by_id(&self, job_id: Uuid) -> Option<Job> {
        let mut q = self.inner.lock().await;
        if let Some(pos) = q.iter().position(|job| job.job_id == job_id) {
            Some(q.remove(pos).unwrap())
        } else {
            None
        }
    }

    pub async fn snapshot(&self) -> Vec<JobSummary> {
        let q = self.inner.lock().await;
        q.iter()
            .map(|j| JobSummary { job_id: j.job_id, submitted_at: j.submitted_at })
            .collect()
    }
}