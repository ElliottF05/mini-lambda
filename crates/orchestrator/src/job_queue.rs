use hashlink::LinkedHashMap;
use shared::WorkerResponse;
use tokio::sync::oneshot;
use uuid::Uuid;

/// FIFO queue of pending jobs, skipping any whose requester has disconnected.
#[derive(Debug)]
pub struct JobQueue {
    inner: LinkedHashMap<Uuid, oneshot::Sender<WorkerResponse>>
}

impl JobQueue {
    /// Create a new, empty JobQueue.
    pub fn new() -> JobQueue {
        JobQueue { inner: LinkedHashMap::new() }
    }

    /// Add a job to the back of the queue.
    pub fn enqueue(&mut self, job_id: Uuid, tx: oneshot::Sender<WorkerResponse>) {
        self.inner.insert(job_id, tx);
    }

    /// Remove and return the next job whose sender is still open, discarding any that have been cancelled.
    pub fn dequeue(&mut self) -> Option<(Uuid, oneshot::Sender<WorkerResponse>)> {
        while let Some((job_id, tx)) = self.inner.pop_front() {
            if !tx.is_closed() {
                return Some((job_id, tx))
            }
        }
        None
    }

    pub fn cancel(&mut self, job_id: &Uuid) -> bool {
        self.inner.remove(job_id).is_some()
    }
}