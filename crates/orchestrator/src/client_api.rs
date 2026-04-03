use tokio::sync::oneshot;
use tonic::{Request, Status, Response};

use shared::client_api_server::ClientApi;
use shared::{CancelJobRequest, CancelJobResponse, WorkerRequest, WorkerResponse};
use uuid::Uuid;

use crate::orchestrator::Orchestrator;
use crate::errors::OrchestratorError;

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
            .map_err(|e| OrchestratorError::MalformedJobId(e.to_string()))?;
        let (tx, rx) = oneshot::channel();

        // Add this job to the queue and dispatch pending jobs atomically
        {
            let mut queue = self.job_queue.lock().await;
            let mut registry = self.registry.lock().await;

            queue.enqueue(job_id, tx);
            Self::dispatch_pending_jobs(&mut queue, &mut registry);
        }

        // Awake when this job is dispatched
        match rx.await {
            Ok(response) => {
                println!("Worker at {} became available, dequeueing job with id {}", response.worker_address, job_id);
                Ok(Response::new(response))
            },
            Err(_) => Err(OrchestratorError::JobCancelled.into())
        }
    }

    /// A function exposed by the Orchestrator for the Client to call
    /// to cancel a job waiting in the Orchestrator queue. 
    /// If this job is in the Orchestrator queue, it will remove it.
    /// Returns an error on invalid job id.
    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>
    ) -> Result<Response<CancelJobResponse>, Status> {
        let job_id = Uuid::from_slice(&request.into_inner().job_id)
            .map_err(|e| OrchestratorError::MalformedJobId(e.to_string()))?;

        if self.job_queue.lock().await.cancel(&job_id) {
            Ok(Response::new(CancelJobResponse {}))
        } else {
            Err(OrchestratorError::JobNotFound.into())
        }
    }
}