use tonic::transport::Server;
use tonic::{Request, Status, Response};

use shared::cli_api_server::{CliApi, CliApiServer};
use shared::{WorkerRequest, WorkerResponse};

#[derive(Debug, Default)]
struct Orchestrator {}

#[tonic::async_trait]
impl CliApi for Orchestrator {

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
    let orchestrator = Orchestrator::default();
    
    Server::builder()
        .add_service(CliApiServer::new(orchestrator))
        .serve(addr)
        .await?;

    Ok(())
}