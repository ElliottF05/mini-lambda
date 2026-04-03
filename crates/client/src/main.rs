use tonic::Request;
use clap::Parser;

use shared::client_api_client::ClientApiClient as ClientApi;
use shared::executor_client::ExecutorClient;
use shared::{JobRequest, WorkerRequest};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(about = "Submit a wasm job to the distributed compute platform")]
struct Args {
    wasm_path: String,
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    orchestrator: String,
    wasm_args: Vec<String>
}

/// The main cli entrypoint to the Client, allowing submission of a wasm job.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();

    let orchestrator_endpoint = args.orchestrator;
    let mut orchestrator_client = ClientApi::connect(orchestrator_endpoint).await
        .unwrap_or_else(|e| panic!("Failed to connect to the Orchestrator: {}", e));

    let job_id = Uuid::new_v4().as_bytes().to_vec();

    let worker_request = WorkerRequest { job_id: job_id.clone() };

    let response = orchestrator_client.request_worker(Request::new(worker_request)).await
        .unwrap_or_else(|e| panic!("Failed to request a worker from the Orchestrator: {}", e));
    let worker_address = response.into_inner().worker_address; 
    let worker_endpoint = "http://".to_string() + &worker_address;
    println!("Worker endpoint: {}", worker_endpoint);

    let wasm_bytes = std::fs::read(args.wasm_path).unwrap();
    let mut executor_client = ExecutorClient::connect(worker_endpoint).await
        .unwrap_or_else(|e| panic!("Failed to connect to the provided Worker: {}", e));
    let job_request = JobRequest { job_id, wasm_bytes, args: args.wasm_args };

    let execution_result = executor_client.execute_job(Request::new(job_request)).await;
    match execution_result {
        Ok(job_response) => {
            println!("Job execution response: {:?}", String::from_utf8(job_response.into_inner().stdout));
        },
        Err(e) => {
            match e.code() {
                tonic::Code::InvalidArgument => {
                    eprintln!("Job produced an error at compilation or runtime: {}", e.message());
                },
                tonic::Code::Internal => {
                    eprintln!("Worker failed with error: {}", e.message());
                }
                _ => {
                    eprintln!("Received an unexepected error code {}, with message: {}", e.code(), e.message());
                }
            }
        }
    }
}