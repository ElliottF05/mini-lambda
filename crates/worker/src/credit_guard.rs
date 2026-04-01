use shared::{CreditUpdate, WorkerMessage, worker_message};

use crate::worker::Worker;

/// RAII guard that returns a credit via an update to the Orchestrator when dropped.
pub struct CreditGuard<'a> {
    worker: &'a Worker
}

impl<'a> CreditGuard<'a> {
    /// Creates a new `CreditGuard` bound to the given Worker.
    pub fn new(worker: &'a Worker) -> Self {
        Self { worker }
    }
}

impl<'a> Drop for CreditGuard<'a> {
    /// Sends a credit update to the Orchestrator, returning one credit.
    fn drop(&mut self) {
        let tx= self.worker.orchestrator_tx.clone();
        tokio::spawn(async move {
            if tx.send(WorkerMessage { 
                message: Some(worker_message::Message::CreditUpdate(CreditUpdate { delta: 1 }))
            }).await.is_err() {
                eprintln!("Lost connection to the Orchestrator, shutting down");
                std::process::exit(1);
            }
        });
    }
}