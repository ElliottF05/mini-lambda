use tonic::Request;

use shared::cli_api_client::CliApiClient;
use shared::executor_client::ExecutorClient;
use shared::{JobRequest, WorkerRequest};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args : Vec<_>= std::env::args().skip(1).collect();
    let wasm_path = &cli_args[0];
    let wasm_args = &cli_args[1..];

    let orchestrator_endpoint: &str = "http://127.0.0.1:50051";
    let mut orchestrator_client = CliApiClient::connect(orchestrator_endpoint).await?;

    let worker_request = WorkerRequest {};

    let response = orchestrator_client.request_worker(Request::new(worker_request)).await?;
    let worker_address = response.into_inner().worker_address; 
    let worker_endpoint = "http://".to_string() + &worker_address;
    println!("Worker endpoint: {}", worker_endpoint);

    let wasm_bytes = std::fs::read(wasm_path).unwrap();
    let mut executor_client = ExecutorClient::connect(worker_endpoint).await?;
    let job_request = JobRequest { wasm_bytes: wasm_bytes, args: wasm_args.to_vec() };
    let response = executor_client.execute_job(Request::new(job_request)).await?;
    println!("Job execution response: {:?}", String::from_utf8(response.into_inner().result));

    Ok(())
}