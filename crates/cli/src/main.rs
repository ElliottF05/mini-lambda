use tonic::Request;

use shared::cli_api_client::CliApiClient;
use shared::WorkerRequest;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://127.0.0.1:50051";
    let mut client = CliApiClient::connect(url).await?;

    let req = WorkerRequest {};
    let request = Request::new(req);

    let response = client.request_worker(request).await?;
    println!("Worker address: {}", response.into_inner().worker_address);

    Ok(())
}