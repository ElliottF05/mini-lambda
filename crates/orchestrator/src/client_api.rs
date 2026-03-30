use tokio::sync::oneshot;
use tonic::{Request, Status, Response};

use shared::client_api_server::ClientApi;
use shared::{WorkerRequest, WorkerResponse};
use uuid::Uuid;

use crate::job_queue::PendingJob;
use crate::orchestrator::Orchestrator;
use crate::errors::ClientError;

/// Implementation of the CliApi service for the Orchestrator.
#[tonic::async_trait]
impl ClientApi for Orchestrator {

    /// A function exposed by the Orchestrator for the client to call to request
    /// a worker be assigned to them to execute their job.
    async fn request_worker(
        &self, 
        request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        // Create the pending job
        let job_id = Uuid::from_slice(&request.into_inner().job_id)
            .map_err(ClientError::InvalidJobId)?;
        let (tx, rx) = oneshot::channel();
        let job = PendingJob { id: job_id, tx };

        // Add this job to the queue and dispatch pending jobs atomically
        {
            let mut queue = self.job_queue.write().await;
            let mut registry = self.registry.write().await;

            queue.enqueue(job);
            Self::dispatch_pending_jobs(&mut queue, &mut registry);
        }

        // Awake when this job is dispatched
        match rx.await {
            Ok(response) => {
                println!("Worker at {} became available, dequeueing job with id {}", response.worker_address, job_id);
                Ok(Response::new(response))
            },
            Err(_) => Err(ClientError::JobCancelled.into())
        }
    }
}