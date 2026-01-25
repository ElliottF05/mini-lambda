use tonic::Request;

use mini_lambda_shared::orchestrator_proto::orchestrator_client::OrchestratorClient;
use mini_lambda_shared::orchestrator_proto::WorkerRequest;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://127.0.0.1:50051";
    let mut client = OrchestratorClient::connect(url).await?;

    let req = WorkerRequest {};
    let request = Request::new(req);

    let response = client.request_worker(request).await?;
    println!("Worker address: {}", response.into_inner().worker_address);

    Ok(())
}