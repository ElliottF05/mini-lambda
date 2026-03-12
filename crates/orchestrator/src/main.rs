mod orchestrator;
mod client_api;
mod worker_api;
mod registry;

use tonic::transport::Server;

use shared::{client_api_server::ClientApiServer, worker_api_server::WorkerApiServer};
use crate::orchestrator::Orchestrator;

/// Main entry point for the Orchestrator server binary.
#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: make this addr a CLI arg passed in
    let addr = "127.0.0.1:50051".parse()?;
    let orchestrator = Orchestrator::default();
    
    println!("Orchestrator listening on {}", addr);
    Server::builder()
        .add_service(ClientApiServer::new(orchestrator.clone()))
        .add_service(WorkerApiServer::new(orchestrator.clone()))
        .serve(addr)
        .await?;

    Ok(())
}