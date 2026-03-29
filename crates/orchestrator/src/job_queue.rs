use std::hash::{Hash, Hasher};

use hashlink::LinkedHashSet;
use shared::WorkerResponse;
use tokio::sync::oneshot;
use uuid::Uuid;

/// A job awaiting dispatch to a Worker.
#[derive(Debug)]
pub struct PendingJob {
    pub id: Uuid,
    pub tx: oneshot::Sender<WorkerResponse>
}

impl Hash for PendingJob {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
impl PartialEq for PendingJob {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for PendingJob {}


/// FIFO queue of pending jobs, skipping any whose requester has disconnected.
#[derive(Debug)]
pub struct JobQueue {
    inner: LinkedHashSet<PendingJob>
}

impl JobQueue {
    /// Create a new, empty JobQueue.
    pub fn new() -> JobQueue {
        JobQueue { inner: LinkedHashSet::new() }
    }

    /// Add a job to the back of the queue.
    pub fn enqueue(&mut self, job: PendingJob) {
        self.inner.insert(job);
    }

    /// Remove and return the next job whose sender is still open, discarding any that have been cancelled.
    pub fn dequeue(&mut self) -> Option<PendingJob> {
        while let Some(job) = self.inner.pop_front() {
            if !job.tx.is_closed() {
                return Some(job)
            }
        }
        None
    }
}