mod orchestrator;
mod client_api;
mod worker_api;
mod registry;
mod job_queue;

use clap::Parser;
use tonic::transport::Server;

use shared::{client_api_server::ClientApiServer, worker_api_server::WorkerApiServer};
use crate::orchestrator::Orchestrator;

#[derive(Parser, Debug)]
#[command(about = "Run the Orchestrator server")]
struct Args {
    #[arg(default_value = "127.0.0.1:50051")]
    addr: std::net::SocketAddr,
}

/// Main entry point for the Orchestrator server binary.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();
    let addr = args.addr;
    let orchestrator = Orchestrator::default();
    
    println!("Orchestrator listening on {}", addr);
    Server::builder()
        .add_service(ClientApiServer::new(orchestrator.clone()))
        .add_service(WorkerApiServer::new(orchestrator.clone()))
        .serve(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to serve the Orchestrator: {}", e));
}