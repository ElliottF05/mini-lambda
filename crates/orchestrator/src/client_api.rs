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

        // Get client address
        let client_address = request.remote_addr()
            .unwrap_or_else(|| {
                tracing::error!("ERROR: couldn't read client address, this should never occur");
                std::process::exit(1);
            })
            .to_string();
        self.diagnostics.handle_client_connected(&client_address);

        // Create the pending job
        let job_id = Uuid::from_slice(&request.into_inner().job_id)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "ERROR: received malformed job_id bytes from the client, this should never occur");
                std::process::exit(1);
            });

        tracing::info!(job_id = %job_id, "job request received");

        let (tx, rx) = oneshot::channel();

        tracing::debug!(job_id = %job_id, "job enqueued, waiting for worker");

        // Add this job to the queue and dispatch pending jobs atomically
        {
            let mut queue = self.job_queue.lock().await;
            let mut registry = self.registry.lock().await;

            self.diagnostics.handle_job_enqueue(job_id, &client_address);

            queue.enqueue(job_id, tx);
            Self::dispatch_pending_jobs(&mut queue, &mut registry, &self.jwt_secret);
        }

        // Awake when this job is dispatched
        match rx.await {
            Ok(response) => {
                tracing::info!(job_id = %job_id, worker = %response.worker_address, "worker assigned");
                self.diagnostics.handle_dispatch_job(job_id, &response.worker_address);
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
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "ERROR: received malformed job_id bytes from the client, this should never occur");
                std::process::exit(1);
            });

        if self.job_queue.lock().await.cancel(&job_id) {
            tracing::debug!(job_id = %job_id, "job cancelled from queue");
            self.diagnostics.handle_cancel_queued_job(job_id);
            Ok(Response::new(CancelJobResponse {}))
        } else {
            tracing::debug!(job_id = %job_id, "cancel requested but job not in queue (may have been dispatched)");
            Err(OrchestratorError::JobNotFound.into())
        }
    }
}

/// Interceptor that verifies the authorization header matches the configured client password.
/// Returns Unauthenticated if the password is wrong or missing. No-op if no password is configured.
pub fn check_client_auth(orchestrator: Orchestrator) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    let password = orchestrator.client_password.clone();
    move |req: Request<()>| {
        if let Some(expected) = &password {
            let actual = req.metadata().get("authorization")
                .and_then(|v| v.to_str().ok());
            if actual != Some(expected) {
                return Err(Status::unauthenticated("invalid client password"));
            }
        }
        Ok(req)
    }
}