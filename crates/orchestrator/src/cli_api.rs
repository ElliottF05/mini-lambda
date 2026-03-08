use tonic::{Request, Status, Response};

use shared::cli_api_server::CliApi;
use shared::{WorkerRequest, WorkerResponse};

use crate::orchestrator::Orchestrator;

/// Implementation of the CliApi service for the Orchestrator.
#[tonic::async_trait]
impl CliApi for Orchestrator {

    async fn request_worker(
        &self, 
        _request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        // TODO: process error here instead of unwrapping it
        let worker = self.registry.read().await.get_worker().unwrap().clone();
        let response = WorkerResponse {
            worker_address: worker.address
        };

        return Ok(Response::new(response));
    }
}