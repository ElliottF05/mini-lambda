mod worker;
mod executor;
mod errors;
mod orchestrator_client;

use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

use shared::executor_server::ExecutorServer;

use crate::worker::Worker;

/// Main entry point for the Worker server binary.
#[tokio::main]
pub async fn main() {
    let cli_args: Vec<_> = std::env::args().skip(1).collect();
    if cli_args.len() != 2 {
        eprintln!("Usage: worker <orchestrator-url> <bind-host>\n  e.g. worker http://127.0.0.1:50051 127.0.0.1");
        std::process::exit(1);
    }
    let orchestrator_endpoint = &cli_args[0];
    let bind_host = &cli_args[1];

    let listener = TcpListener::bind(format!("{}:0", bind_host)).await
        .unwrap_or_else(|e| panic!("Failed to bind to host {}: {}", bind_host, e));
    let addr = listener.local_addr()
        .unwrap_or_else(|e| panic!("Failed to fetch port Worker is bound to: {}", e));

    // Register this worker with the orchestrator
    let worker = Worker::new(addr, orchestrator_endpoint).await;
    
    // Start the executor server
    println!("Worker listening on {}", addr);
    let incoming = TcpListenerStream::new(listener);
    Server::builder()
        .add_service(ExecutorServer::new(worker))
        .serve_with_incoming_shutdown(incoming, async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .unwrap_or_else(|e| panic!("Executor server failed: {}", e));
}