use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

use shared::executor_server::ExecutorServer;

use crate::worker::Worker;

mod worker;
mod executor;
mod errors;

/// Main entry point for the Worker server binary.
#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Register this worker with the orchestrator
    // TODO: accept orchestrator endpoint as cli arg
    let orchestrator_url = "http://127.0.0.1:50051";
    let worker = Worker::new(addr, orchestrator_url).await;
    
    // Start the executor server
    println!("Worker listening on {}", addr);
    let _server_handle = tokio::spawn(async move {
        let incoming = TcpListenerStream::new(listener);
        Server::builder()
            .add_service(ExecutorServer::new(worker))
            .serve_with_incoming(incoming)
            .await
    });

    // Keep the main task alive
    tokio::signal::ctrl_c().await?;

    Ok(())
}