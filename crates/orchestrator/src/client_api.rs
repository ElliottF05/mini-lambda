use tonic::{Request, Status, Response};

use shared::client_api_server::ClientApi;
use shared::{WorkerRequest, WorkerResponse};

use crate::orchestrator::Orchestrator;

/// Implementation of the CliApi service for the Orchestrator.
#[tonic::async_trait]
impl ClientApi for Orchestrator {

    /// A function exposed by the Orchestrator for the lient to call to request
    /// a worker be assigned to them to execute their job.
    async fn request_worker(
        &self, 
        _request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        // TODO: process error here instead of unwrapping it
        let worker_address = self.registry.write().await.get_worker().unwrap().clone();
        let response = WorkerResponse { worker_address };
        return Ok(Response::new(response));
    }
}