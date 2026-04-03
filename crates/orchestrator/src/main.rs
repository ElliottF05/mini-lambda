mod orchestrator;
mod client_api;
mod worker_api;
mod registry;
mod job_queue;
mod errors;

use clap::Parser;
use tonic::transport::Server;

use shared::{client_api_server::ClientApiServer, worker_api_server::WorkerApiServer};
use crate::{client_api::check_client_auth, orchestrator::Orchestrator, worker_api::check_worker_auth};

#[derive(Parser, Debug)]
#[command(about = "Run the Orchestrator server")]
struct Args {
    #[arg(default_value = "127.0.0.1:50051")]
    addr: std::net::SocketAddr,
    #[arg(long, help = "Password required for workers to register. If not set, no password is required.")]
    worker_password: Option<String>,
    #[arg(long, help = "Password required for clients to submit jobs. If not set, no password is required.")]
    client_password: Option<String>,
}

/// Main entry point for the Orchestrator server binary.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();
    let addr = args.addr;
    let orchestrator = Orchestrator::new(args.worker_password, args.client_password);

    let client_server = ClientApiServer::with_interceptor(orchestrator.clone(), check_client_auth(orchestrator.clone()));
    let worker_server = WorkerApiServer::with_interceptor(orchestrator.clone(), check_worker_auth(orchestrator.clone()));
    
    println!("Orchestrator listening on {}", addr);
    Server::builder()
        .add_service(client_server)
        .add_service(worker_server)
        .serve(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to serve the Orchestrator: {}", e));
}