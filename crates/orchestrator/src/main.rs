mod orchestrator;
mod client_api;
mod worker_api;
mod registry;

use tonic::transport::Server;

use shared::{client_api_server::ClientApiServer, worker_api_server::WorkerApiServer};
use crate::orchestrator::Orchestrator;

/// Main entry point for the Orchestrator server binary.
#[tokio::main]
pub async fn main() {
    let cli_args: Vec<_> = std::env::args().skip(1).collect();
    if cli_args.len() != 1 {
        eprintln!("Usage: orchestrator <orchestrator-address> e.g. orchestrator 127.0.0.1:50051");
        std::process::exit(1);
    }
    let addr = cli_args[0].parse()
        .unwrap_or_else(|e| panic!("Failed to parse the given addr {}: {}", cli_args[0], e));
    let orchestrator = Orchestrator::default();
    
    println!("Orchestrator listening on {}", addr);
    Server::builder()
        .add_service(ClientApiServer::new(orchestrator.clone()))
        .add_service(WorkerApiServer::new(orchestrator.clone()))
        .serve(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to serve the Orchestrator: {}", e));
}