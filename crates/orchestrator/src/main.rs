use tonic::transport::Server;
use tonic::{Request, Status, Response};

use mini_lambda_shared::orchestrator_proto::orchestrator_server::{Orchestrator, OrchestratorServer};
use mini_lambda_shared::orchestrator_proto::{WorkerRequest, WorkerResponse};

#[derive(Debug, Default)]
struct OrchestratorService {}

#[tonic::async_trait]
impl Orchestrator for OrchestratorService {

    async fn request_worker(
        &self, 
        _request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        let reponse = WorkerResponse {
            worker_address: "worker-1-address".to_string(),
        };

        return Ok(Response::new(reponse));
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse().unwrap();
    let orchestrator_service = OrchestratorService::default();
    
    Server::builder()
        .add_service(OrchestratorServer::new(orchestrator_service))
        .serve(addr)
        .await?;

    Ok(())
}